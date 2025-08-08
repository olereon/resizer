//! Parallel processing utilities for high-performance batch operations

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tracing::{info, debug};
use rayon::prelude::*;

use crate::config::ResizeConfig;
use crate::processing::{ProcessingEngine, ProcessingResult};
use crate::error::{Result, FastResizeError};

pub mod progress;
pub mod scheduler;

pub use progress::*;
pub use scheduler::*;

/// Parallel batch processor for handling multiple images efficiently
pub struct ParallelProcessor {
    engine: Arc<ProcessingEngine>,
    max_concurrent: usize,
    progress_tracker: Arc<ProgressTracker>,
    semaphore: Arc<Semaphore>,
}

impl ParallelProcessor {
    /// Create a new parallel processor
    pub fn new(max_concurrent: Option<usize>) -> Self {
        let max_concurrent = max_concurrent.unwrap_or_else(|| {
            // Use number of logical CPUs, but cap at 16 to avoid excessive memory usage
            num_cpus::get().min(16)
        });

        info!("Initializing parallel processor with {} concurrent workers", max_concurrent);

        Self {
            engine: Arc::new(ProcessingEngine::new()),
            max_concurrent,
            progress_tracker: Arc::new(ProgressTracker::new()),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Process a batch of files in parallel
    pub async fn process_batch(
        &self,
        files: Vec<PathBuf>,
        output_dir: &std::path::Path,
        config: &ResizeConfig,
    ) -> Result<BatchProcessingResult> {
        let start_time = Instant::now();
        let total_files = files.len();
        
        info!("Starting parallel processing of {} files", total_files);
        
        // Initialize progress tracking
        self.progress_tracker.start(total_files as u64);
        
        // Create output directory if it doesn't exist
        tokio::fs::create_dir_all(output_dir).await
            .map_err(|e| FastResizeError::system(format!("Failed to create output directory: {}", e)))?;

        // Process files using async/await with semaphore for concurrency control
        let results = self.process_files_async(files, output_dir, config).await;

        let processing_time = start_time.elapsed();
        let batch_result = self.aggregate_results(results, processing_time);
        
        info!("Parallel processing completed in {:.2}s", processing_time.as_secs_f64());
        
        Ok(batch_result)
    }

    /// Process files using CPU-bound thread pool
    pub fn process_batch_cpu_intensive(
        &self,
        files: Vec<PathBuf>,
        output_dir: &std::path::Path,
        config: &ResizeConfig,
    ) -> Result<BatchProcessingResult> {
        let start_time = Instant::now();
        let total_files = files.len();
        
        info!("Starting CPU-intensive parallel processing of {} files", total_files);
        
        // Use rayon for CPU-bound parallelism
        let results: Vec<Result<ProcessingResult>> = files
            .par_iter()
            .map(|file_path| {
                let output_path = self.generate_output_path(file_path, output_dir, config);
                
                // Use blocking runtime for CPU-intensive work
                let runtime = tokio::runtime::Handle::current();
                runtime.block_on(async {
                    self.process_single_file(file_path, &output_path, config).await
                })
            })
            .collect();

        let processing_time = start_time.elapsed();
        let batch_result = self.aggregate_results(results, processing_time);
        
        info!("CPU-intensive processing completed in {:.2}s", processing_time.as_secs_f64());
        
        Ok(batch_result)
    }

    /// Process files with hybrid approach (I/O async, CPU parallel)
    pub async fn process_batch_hybrid(
        &self,
        files: Vec<PathBuf>,
        output_dir: &std::path::Path,
        config: &ResizeConfig,
    ) -> Result<BatchProcessingResult> {
        let start_time = Instant::now();
        let total_files = files.len();
        
        info!("Starting hybrid parallel processing of {} files", total_files);
        
        // Split into chunks for better memory management
        let chunk_size = (total_files / self.max_concurrent).max(1).min(10);
        let chunks: Vec<_> = files.chunks(chunk_size).collect();
        
        let mut all_results = Vec::new();
        
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            debug!("Processing chunk {} of {} ({} files)", 
                  chunk_idx + 1, chunks.len(), chunk.len());
            
            // Process chunk with controlled concurrency
            let chunk_results = self.process_chunk_async(chunk, output_dir, config).await;
            all_results.extend(chunk_results);
            
            // Small delay between chunks to allow memory cleanup
            if chunk_idx < chunks.len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }

        let processing_time = start_time.elapsed();
        let batch_result = self.aggregate_results(all_results, processing_time);
        
        info!("Hybrid processing completed in {:.2}s", processing_time.as_secs_f64());
        
        Ok(batch_result)
    }

    /// Process files asynchronously with semaphore-based concurrency control
    async fn process_files_async(
        &self,
        files: Vec<PathBuf>,
        output_dir: &std::path::Path,
        config: &ResizeConfig,
    ) -> Vec<Result<ProcessingResult>> {
        let mut tasks = Vec::new();
        
        for file_path in files {
            let engine = Arc::clone(&self.engine);
            let semaphore = Arc::clone(&self.semaphore) as Arc<Semaphore>;
            let progress_tracker = Arc::clone(&self.progress_tracker);
            let output_path = self.generate_output_path(&file_path, output_dir, config);
            let config = config.clone();
            
            let task = tokio::spawn(async move {
                // Acquire semaphore permit
                let _permit = semaphore.acquire().await.unwrap();
                
                // Update progress
                progress_tracker.start_file(file_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string());
                
                // Process file
                let result = engine.process_file(&file_path, &output_path, &config).await;
                
                // Update progress
                match &result {
                    Ok(_) => progress_tracker.complete_file(true),
                    Err(e) => {
                        progress_tracker.complete_file(false);
                        debug!("Failed to process {:?}: {}", file_path, e);
                    }
                }
                
                result
            });
            
            tasks.push(task);
        }
        
        // Wait for all tasks to complete
        let results = futures::future::join_all(tasks).await;
        
        // Extract results, handling task join errors
        results.into_iter()
            .map(|task_result| {
                task_result.map_err(|e| FastResizeError::system(format!("Task join error: {}", e)))?
            })
            .collect()
    }

    /// Process a chunk of files asynchronously
    async fn process_chunk_async(
        &self,
        chunk: &[PathBuf],
        output_dir: &std::path::Path,
        config: &ResizeConfig,
    ) -> Vec<Result<ProcessingResult>> {
        let mut tasks = Vec::new();
        
        for file_path in chunk {
            let engine = Arc::clone(&self.engine);
            let output_path = self.generate_output_path(file_path, output_dir, config);
            let config = config.clone();
            let file_path = file_path.clone();
            
            let task = tokio::spawn(async move {
                engine.process_file(&file_path, &output_path, &config).await
            });
            
            tasks.push(task);
        }
        
        // Wait for chunk completion
        let results = futures::future::join_all(tasks).await;
        
        results.into_iter()
            .map(|task_result| {
                task_result.map_err(|e| FastResizeError::system(format!("Task join error: {}", e)))?
            })
            .collect()
    }

    /// Process a single file (used by parallel executors)
    async fn process_single_file(
        &self,
        input_path: &std::path::Path,
        output_path: &std::path::Path,
        config: &ResizeConfig,
    ) -> Result<ProcessingResult> {
        self.engine.process_file(input_path, output_path, config).await
    }

    /// Generate output path for a file
    fn generate_output_path(
        &self,
        input_path: &std::path::Path,
        output_dir: &std::path::Path,
        config: &ResizeConfig,
    ) -> PathBuf {
        let file_name = input_path.file_name().unwrap();
        let mut output_path = output_dir.join(file_name);
        
        // Change extension if format conversion is specified
        if let Some(format) = config.format {
            output_path.set_extension(format.extension());
        }
        
        output_path
    }

    /// Aggregate processing results
    fn aggregate_results(
        &self,
        results: Vec<Result<ProcessingResult>>,
        processing_time: std::time::Duration,
    ) -> BatchProcessingResult {
        let mut successful_results = Vec::new();
        let mut failed_results = Vec::new();
        
        let mut total_input_size = 0u64;
        let mut total_output_size = 0u64;
        let mut total_pixels_processed = 0u64;
        
        for result in results {
            match result {
                Ok(processing_result) => {
                    total_input_size += processing_result.original_info.file_size;
                    total_output_size += processing_result.output_info.file_size;
                    total_pixels_processed += processing_result.original_info.pixel_count;
                    successful_results.push(processing_result);
                }
                Err(error) => {
                    failed_results.push(error);
                }
            }
        }
        
        BatchProcessingResult {
            successful: successful_results.len() as u32,
            failed: failed_results.len() as u32,
            successful_results: successful_results.clone(),
            failed_errors: failed_results,
            processing_time,
            total_input_size,
            total_output_size,
            total_pixels_processed,
            files_per_second: successful_results.len() as f64 / processing_time.as_secs_f64(),
            pixels_per_second: total_pixels_processed as f64 / processing_time.as_secs_f64(),
        }
    }

    /// Get current progress
    pub fn get_progress(&self) -> ProgressState {
        self.progress_tracker.get_state()
    }
}

/// Result of batch processing operation
#[derive(Debug)]
pub struct BatchProcessingResult {
    pub successful: u32,
    pub failed: u32,
    pub successful_results: Vec<ProcessingResult>,
    pub failed_errors: Vec<FastResizeError>,
    pub processing_time: std::time::Duration,
    pub total_input_size: u64,
    pub total_output_size: u64,
    pub total_pixels_processed: u64,
    pub files_per_second: f64,
    pub pixels_per_second: f64,
}

impl BatchProcessingResult {
    /// Get compression ratio
    pub fn compression_ratio(&self) -> f64 {
        if self.total_output_size == 0 {
            return 1.0;
        }
        self.total_input_size as f64 / self.total_output_size as f64
    }

    /// Get size reduction percentage
    pub fn size_reduction(&self) -> f64 {
        if self.total_input_size == 0 {
            return 0.0;
        }
        let reduction = self.total_input_size.saturating_sub(self.total_output_size);
        (reduction as f64 / self.total_input_size as f64) * 100.0
    }

    /// Get average processing time per file
    pub fn average_time_per_file(&self) -> std::time::Duration {
        if self.successful == 0 {
            return std::time::Duration::from_secs(0);
        }
        self.processing_time / self.successful
    }

    /// Print summary to console
    pub fn print_summary(&self) {
        println!("Batch Processing Results:");
        println!("  Successful: {}", self.successful);
        if self.failed > 0 {
            println!("  Failed: {}", self.failed);
        }
        println!("  Duration: {:.2}s", self.processing_time.as_secs_f64());
        
        if self.successful > 0 {
            println!("  Speed: {:.1} files/sec, {:.0} pixels/sec", 
                    self.files_per_second, self.pixels_per_second);
            println!("  Size: {:.2}MB → {:.2}MB (compression: {:.1}x, reduction: {:.1}%)",
                    self.total_input_size as f64 / 1024.0 / 1024.0,
                    self.total_output_size as f64 / 1024.0 / 1024.0,
                    self.compression_ratio(),
                    self.size_reduction());
        }
        
        // Print individual errors if any
        if !self.failed_errors.is_empty() {
            println!("\nErrors:");
            for (i, error) in self.failed_errors.iter().enumerate() {
                println!("  {}: {}", i + 1, error);
            }
        }
    }
}

/// Strategy for parallel processing
#[derive(Debug, Clone, Copy)]
pub enum ProcessingStrategy {
    /// Pure async processing (good for I/O bound operations)
    Async,
    /// CPU-intensive processing using thread pool
    CpuIntensive,
    /// Hybrid approach balancing I/O and CPU usage
    Hybrid,
    /// Automatically choose based on file count and system resources
    Auto,
}

impl ProcessingStrategy {
    /// Choose appropriate strategy based on context
    pub fn choose_auto(file_count: usize, available_memory: u64) -> Self {
        if file_count < 10 {
            Self::Async
        } else if available_memory < 2 * 1024 * 1024 * 1024 { // < 2GB
            Self::Hybrid
        } else if file_count > 100 {
            Self::CpuIntensive
        } else {
            Self::Hybrid
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::config::ResizeMode;

    #[tokio::test]
    async fn test_parallel_processor_creation() {
        let processor = ParallelProcessor::new(Some(4));
        assert_eq!(processor.max_concurrent, 4);

        let auto_processor = ParallelProcessor::new(None);
        assert!(auto_processor.max_concurrent > 0);
        assert!(auto_processor.max_concurrent <= 16);
    }

    #[test]
    fn test_processing_strategy_auto() {
        // Small batch
        let strategy = ProcessingStrategy::choose_auto(5, 8 * 1024 * 1024 * 1024);
        assert!(matches!(strategy, ProcessingStrategy::Async));

        // Low memory
        let strategy = ProcessingStrategy::choose_auto(50, 1 * 1024 * 1024 * 1024);
        assert!(matches!(strategy, ProcessingStrategy::Hybrid));

        // Large batch
        let strategy = ProcessingStrategy::choose_auto(150, 8 * 1024 * 1024 * 1024);
        assert!(matches!(strategy, ProcessingStrategy::CpuIntensive));
    }

    #[tokio::test]
    async fn test_batch_processing_result() {
        let result = BatchProcessingResult {
            successful: 10,
            failed: 2,
            successful_results: Vec::new(),
            failed_errors: Vec::new(),
            processing_time: std::time::Duration::from_secs(5),
            total_input_size: 10_000_000,
            total_output_size: 5_000_000,
            total_pixels_processed: 1_000_000,
            files_per_second: 2.0,
            pixels_per_second: 200_000.0,
        };

        assert_eq!(result.compression_ratio(), 2.0);
        assert_eq!(result.size_reduction(), 50.0);
        assert_eq!(result.average_time_per_file(), std::time::Duration::from_millis(500));
    }

    #[tokio::test]
    async fn test_output_path_generation() {
        let processor = ParallelProcessor::new(Some(1));
        let config = ResizeConfig {
            mode: ResizeMode::Scale { factor: 0.5 },
            quality: 90,
            format: Some(crate::config::ImageFormat::WebP),
        };

        let input_path = std::path::Path::new("test.jpg");
        let output_dir = std::path::Path::new("/output");
        
        let output_path = processor.generate_output_path(input_path, output_dir, &config);
        assert_eq!(output_path.extension().unwrap(), "webp");
        assert_eq!(output_path.file_stem().unwrap(), "test");
    }
}