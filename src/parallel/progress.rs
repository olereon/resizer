//! Progress tracking for parallel operations

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{debug, info};

/// Thread-safe progress tracker for parallel operations
pub struct ProgressTracker {
    state: Arc<Mutex<ProgressState>>,
    sender: broadcast::Sender<ProgressUpdate>,
    start_time: Arc<Mutex<Option<Instant>>>,
    
    // Atomic counters for high-frequency updates
    completed: AtomicUsize,
    failed: AtomicUsize,
    bytes_processed: AtomicU64,
    pixels_processed: AtomicU64,
}

/// Current progress state
#[derive(Debug, Clone)]
pub struct ProgressState {
    pub total_files: u64,
    pub completed_files: usize,
    pub failed_files: usize,
    pub current_file: Option<String>,
    pub elapsed_time: Duration,
    pub estimated_remaining: Option<Duration>,
    pub bytes_processed: u64,
    pub pixels_processed: u64,
    pub files_per_second: f64,
    pub completion_percentage: f64,
}

/// Progress update event
#[derive(Debug, Clone)]
pub enum ProgressUpdate {
    Started {
        total_files: u64,
    },
    FileStarted {
        filename: String,
    },
    FileCompleted {
        filename: String,
        success: bool,
        file_size: u64,
        pixels: u64,
        processing_time: Duration,
    },
    BatchCompleted {
        final_state: ProgressState,
    },
    Error {
        filename: String,
        error: String,
    },
}

impl ProgressTracker {
    /// Create a new progress tracker
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        
        Self {
            state: Arc::new(Mutex::new(ProgressState::new())),
            sender,
            start_time: Arc::new(Mutex::new(None)),
            completed: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
            bytes_processed: AtomicU64::new(0),
            pixels_processed: AtomicU64::new(0),
        }
    }

    /// Start tracking progress for a batch
    pub fn start(&self, total_files: u64) {
        let mut start_time = self.start_time.lock().unwrap();
        *start_time = Some(Instant::now());
        
        let mut state = self.state.lock().unwrap();
        state.total_files = total_files;
        state.completed_files = 0;
        state.failed_files = 0;
        state.current_file = None;
        state.elapsed_time = Duration::from_secs(0);
        state.estimated_remaining = None;
        state.bytes_processed = 0;
        state.pixels_processed = 0;
        state.files_per_second = 0.0;
        state.completion_percentage = 0.0;
        
        // Reset atomic counters
        self.completed.store(0, Ordering::Relaxed);
        self.failed.store(0, Ordering::Relaxed);
        self.bytes_processed.store(0, Ordering::Relaxed);
        self.pixels_processed.store(0, Ordering::Relaxed);

        let _ = self.sender.send(ProgressUpdate::Started { total_files });
        
        info!("Started progress tracking for {} files", total_files);
    }

    /// Mark a file as started
    pub fn start_file(&self, filename: String) {
        let mut state = self.state.lock().unwrap();
        state.current_file = Some(filename.clone());
        
        let _ = self.sender.send(ProgressUpdate::FileStarted { filename });
        
        debug!("Started processing file: {}", state.current_file.as_ref().unwrap());
    }

    /// Mark a file as completed
    pub fn complete_file(&self, success: bool) {
        self.complete_file_with_details(success, 0, 0, Duration::from_secs(0));
    }

    /// Mark a file as completed with detailed information
    pub fn complete_file_with_details(
        &self, 
        success: bool, 
        file_size: u64, 
        pixels: u64, 
        processing_time: Duration
    ) {
        let filename = {
            let mut state = self.state.lock().unwrap();
            let filename = state.current_file.take().unwrap_or_else(|| "unknown".to_string());
            filename
        };

        if success {
            self.completed.fetch_add(1, Ordering::Relaxed);
            self.bytes_processed.fetch_add(file_size, Ordering::Relaxed);
            self.pixels_processed.fetch_add(pixels, Ordering::Relaxed);
        } else {
            self.failed.fetch_add(1, Ordering::Relaxed);
        }

        // Update calculated fields
        self.update_state();

        let _ = self.sender.send(ProgressUpdate::FileCompleted {
            filename: filename.clone(),
            success,
            file_size,
            pixels,
            processing_time,
        });

        debug!("Completed processing file: {} (success: {})", filename, success);
    }

    /// Report an error for a specific file
    pub fn report_error(&self, filename: String, error: String) {
        self.failed.fetch_add(1, Ordering::Relaxed);
        self.update_state();
        
        let _ = self.sender.send(ProgressUpdate::Error { filename, error });
    }

    /// Update calculated state fields
    fn update_state(&self) {
        let start_time = self.start_time.lock().unwrap();
        if start_time.is_none() {
            return;
        }

        let elapsed = start_time.unwrap().elapsed();
        let completed = self.completed.load(Ordering::Relaxed);
        let failed = self.failed.load(Ordering::Relaxed);
        let bytes_processed = self.bytes_processed.load(Ordering::Relaxed);
        let pixels_processed = self.pixels_processed.load(Ordering::Relaxed);

        let mut state = self.state.lock().unwrap();
        state.completed_files = completed;
        state.failed_files = failed;
        state.elapsed_time = elapsed;
        state.bytes_processed = bytes_processed;
        state.pixels_processed = pixels_processed;

        let total_processed = completed + failed;
        if state.total_files > 0 {
            state.completion_percentage = (total_processed as f64 / state.total_files as f64) * 100.0;
        }

        if elapsed.as_secs_f64() > 0.0 {
            state.files_per_second = total_processed as f64 / elapsed.as_secs_f64();
            
            // Estimate remaining time
            if total_processed > 0 && state.total_files > total_processed as u64 {
                let remaining_files = state.total_files - total_processed as u64;
                let avg_time_per_file = elapsed.as_secs_f64() / total_processed as f64;
                let estimated_seconds = remaining_files as f64 * avg_time_per_file;
                state.estimated_remaining = Some(Duration::from_secs_f64(estimated_seconds));
            }
        }
    }

    /// Get current progress state
    pub fn get_state(&self) -> ProgressState {
        self.update_state();
        self.state.lock().unwrap().clone()
    }

    /// Subscribe to progress updates
    pub fn subscribe(&self) -> broadcast::Receiver<ProgressUpdate> {
        self.sender.subscribe()
    }

    /// Mark batch as completed
    pub fn complete_batch(&self) {
        self.update_state();
        let final_state = self.get_state();
        
        let _ = self.sender.send(ProgressUpdate::BatchCompleted { 
            final_state: final_state.clone() 
        });
        
        info!("Batch processing completed: {}/{} files successful in {:.2}s",
              final_state.completed_files,
              final_state.total_files,
              final_state.elapsed_time.as_secs_f64());
    }

    /// Get performance metrics
    pub fn get_metrics(&self) -> ProgressMetrics {
        let state = self.get_state();
        
        ProgressMetrics {
            files_per_second: state.files_per_second,
            bytes_per_second: if state.elapsed_time.as_secs_f64() > 0.0 {
                state.bytes_processed as f64 / state.elapsed_time.as_secs_f64()
            } else {
                0.0
            },
            pixels_per_second: if state.elapsed_time.as_secs_f64() > 0.0 {
                state.pixels_processed as f64 / state.elapsed_time.as_secs_f64()
            } else {
                0.0
            },
            average_file_size: if state.completed_files > 0 {
                state.bytes_processed / state.completed_files as u64
            } else {
                0
            },
            success_rate: if state.total_files > 0 {
                (state.completed_files as f64 / state.total_files as f64) * 100.0
            } else {
                0.0
            },
        }
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressState {
    fn new() -> Self {
        Self {
            total_files: 0,
            completed_files: 0,
            failed_files: 0,
            current_file: None,
            elapsed_time: Duration::from_secs(0),
            estimated_remaining: None,
            bytes_processed: 0,
            pixels_processed: 0,
            files_per_second: 0.0,
            completion_percentage: 0.0,
        }
    }

    /// Get human-readable completion status
    pub fn status_text(&self) -> String {
        if let Some(current) = &self.current_file {
            format!("Processing: {} ({}/{})", current, self.completed_files + self.failed_files + 1, self.total_files)
        } else if self.completion_percentage >= 100.0 {
            "Completed".to_string()
        } else {
            format!("{}/{} files processed", self.completed_files + self.failed_files, self.total_files)
        }
    }

    /// Get estimated time remaining as human-readable string
    pub fn eta_text(&self) -> String {
        match self.estimated_remaining {
            Some(duration) => {
                let seconds = duration.as_secs();
                if seconds < 60 {
                    format!("{}s", seconds)
                } else if seconds < 3600 {
                    format!("{}m {}s", seconds / 60, seconds % 60)
                } else {
                    format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
                }
            }
            None => "Unknown".to_string(),
        }
    }

    /// Get processing speed as human-readable string
    pub fn speed_text(&self) -> String {
        if self.files_per_second >= 1.0 {
            format!("{:.1} files/sec", self.files_per_second)
        } else if self.files_per_second > 0.0 {
            format!("{:.1} sec/file", 1.0 / self.files_per_second)
        } else {
            "Unknown".to_string()
        }
    }
}

/// Performance metrics derived from progress state
#[derive(Debug, Clone)]
pub struct ProgressMetrics {
    pub files_per_second: f64,
    pub bytes_per_second: f64,
    pub pixels_per_second: f64,
    pub average_file_size: u64,
    pub success_rate: f64,
}

impl ProgressMetrics {
    /// Get throughput as human-readable string
    pub fn throughput_text(&self) -> String {
        let mb_per_sec = self.bytes_per_second / 1024.0 / 1024.0;
        format!("{:.1} MB/s, {:.0} Mpx/s", mb_per_sec, self.pixels_per_second / 1_000_000.0)
    }

    /// Get average file size as human-readable string
    pub fn average_size_text(&self) -> String {
        let mb = self.average_file_size as f64 / 1024.0 / 1024.0;
        if mb >= 1.0 {
            format!("{:.1} MB", mb)
        } else {
            format!("{:.0} KB", self.average_file_size as f64 / 1024.0)
        }
    }
}

/// Console progress reporter
pub struct ConsoleProgressReporter {
    receiver: broadcast::Receiver<ProgressUpdate>,
    show_details: bool,
}

impl ConsoleProgressReporter {
    /// Create a new console progress reporter
    pub fn new(tracker: &ProgressTracker, show_details: bool) -> Self {
        Self {
            receiver: tracker.subscribe(),
            show_details,
        }
    }

    /// Start reporting progress to console
    pub async fn start_reporting(&mut self) {
        while let Ok(update) = self.receiver.recv().await {
            match update {
                ProgressUpdate::Started { total_files } => {
                    println!("Starting batch processing of {} files...", total_files);
                }
                ProgressUpdate::FileStarted { filename } => {
                    if self.show_details {
                        println!("Processing: {}", filename);
                    }
                }
                ProgressUpdate::FileCompleted { filename, success, file_size, pixels, processing_time } => {
                    if self.show_details {
                        if success {
                            println!("✓ {} ({:.2} MB, {:.0}K pixels, {:.2}s)", 
                                   filename,
                                   file_size as f64 / 1024.0 / 1024.0,
                                   pixels as f64 / 1000.0,
                                   processing_time.as_secs_f64());
                        } else {
                            println!("✗ {} (failed)", filename);
                        }
                    }
                }
                ProgressUpdate::Error { filename, error } => {
                    eprintln!("Error processing {}: {}", filename, error);
                }
                ProgressUpdate::BatchCompleted { final_state } => {
                    println!("\nBatch processing completed:");
                    println!("  Successful: {}", final_state.completed_files);
                    if final_state.failed_files > 0 {
                        println!("  Failed: {}", final_state.failed_files);
                    }
                    println!("  Duration: {:.2}s", final_state.elapsed_time.as_secs_f64());
                    println!("  Speed: {}", final_state.speed_text());
                    
                    if final_state.bytes_processed > 0 {
                        println!("  Data processed: {:.2} MB", 
                               final_state.bytes_processed as f64 / 1024.0 / 1024.0);
                    }
                    
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_progress_tracker_basic() {
        let tracker = ProgressTracker::new();
        
        tracker.start(10);
        let state = tracker.get_state();
        assert_eq!(state.total_files, 10);
        assert_eq!(state.completed_files, 0);

        tracker.start_file("test1.jpg".to_string());
        tracker.complete_file(true);
        
        let state = tracker.get_state();
        assert_eq!(state.completed_files, 1);
        assert_eq!(state.completion_percentage, 10.0);
    }

    #[tokio::test]
    async fn test_progress_updates() {
        let tracker = ProgressTracker::new();
        let mut receiver = tracker.subscribe();
        
        tracker.start(5);
        
        // Check for Started update
        let update = receiver.recv().await.unwrap();
        assert!(matches!(update, ProgressUpdate::Started { total_files: 5 }));
        
        tracker.start_file("test.jpg".to_string());
        let update = receiver.recv().await.unwrap();
        assert!(matches!(update, ProgressUpdate::FileStarted { .. }));
        
        tracker.complete_file_with_details(true, 1024, 800*600, Duration::from_millis(100));
        let update = receiver.recv().await.unwrap();
        assert!(matches!(update, ProgressUpdate::FileCompleted { success: true, .. }));
    }

    #[test]
    fn test_progress_state_methods() {
        let mut state = ProgressState::new();
        state.total_files = 10;
        state.completed_files = 3;
        state.failed_files = 1;
        state.files_per_second = 2.5;
        
        let status = state.status_text();
        assert!(status.contains("4/10"));
        
        let speed = state.speed_text();
        assert!(speed.contains("2.5"));
    }

    #[test]
    fn test_progress_metrics() {
        let metrics = ProgressMetrics {
            files_per_second: 1.5,
            bytes_per_second: 5_000_000.0, // 5MB/s
            pixels_per_second: 10_000_000.0, // 10M pixels/s
            average_file_size: 2_048_000, // ~2MB
            success_rate: 95.0,
        };

        let throughput = metrics.throughput_text();
        assert!(throughput.contains("4.8 MB/s")); // ~5MB/s
        assert!(throughput.contains("10 Mpx/s"));

        let avg_size = metrics.average_size_text();
        assert!(avg_size.contains("2.0 MB"));
    }

    #[tokio::test]
    async fn test_error_reporting() {
        let tracker = ProgressTracker::new();
        let mut receiver = tracker.subscribe();
        
        tracker.report_error("bad_file.jpg".to_string(), "Corrupted image".to_string());
        
        let update = receiver.recv().await.unwrap();
        if let ProgressUpdate::Error { filename, error } = update {
            assert_eq!(filename, "bad_file.jpg");
            assert_eq!(error, "Corrupted image");
        } else {
            panic!("Expected Error update");
        }
        
        let state = tracker.get_state();
        assert_eq!(state.failed_files, 1);
    }
}