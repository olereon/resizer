//! Intelligent job scheduling for optimal resource utilization

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, warn, info};

use crate::processing::memory::MemoryMonitor;
use crate::error::{Result, FastResizeError};

/// Intelligent scheduler for managing parallel processing workloads
pub struct WorkScheduler {
    memory_monitor: Arc<MemoryMonitor>,
    queue: Arc<Mutex<WorkQueue>>,
    semaphore: Arc<Semaphore>,
    config: SchedulerConfig,
    stats: Arc<Mutex<SchedulerStats>>,
}

/// Configuration for the work scheduler
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum concurrent jobs
    pub max_concurrent: usize,
    /// Target memory usage percentage (0-100)
    pub target_memory_usage: f64,
    /// Batch size for grouping small files
    pub batch_size: usize,
    /// Priority boost for large files
    pub large_file_priority_boost: i32,
    /// Threshold for considering a file "large" (in bytes)
    pub large_file_threshold: u64,
    /// Maximum time to wait for a job slot (in seconds)
    pub max_wait_time: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: num_cpus::get().min(16),
            target_memory_usage: 75.0,
            batch_size: 10,
            large_file_priority_boost: 10,
            large_file_threshold: 50 * 1024 * 1024, // 50MB
            max_wait_time: 300, // 5 minutes
        }
    }
}

/// Work queue for managing job prioritization
struct WorkQueue {
    high_priority: VecDeque<WorkItem>,
    normal_priority: VecDeque<WorkItem>,
    low_priority: VecDeque<WorkItem>,
    total_items: usize,
}

/// Individual work item in the queue
#[derive(Debug, Clone)]
pub struct WorkItem {
    pub id: u64,
    pub input_path: PathBuf,
    pub estimated_size: u64,
    pub priority: JobPriority,
    pub created_at: Instant,
    pub estimated_memory: u64,
    pub estimated_processing_time: Duration,
}

/// Job priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JobPriority {
    Low = 0,
    Normal = 1,
    High = 2,
}

/// Scheduler statistics for monitoring and optimization
#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    pub jobs_queued: u64,
    pub jobs_completed: u64,
    pub jobs_failed: u64,
    pub total_wait_time: Duration,
    pub total_processing_time: Duration,
    pub average_queue_length: f64,
    pub memory_pressure_events: u32,
    pub throughput_files_per_second: f64,
}

impl WorkScheduler {
    /// Create a new work scheduler
    pub fn new(memory_monitor: Arc<MemoryMonitor>, config: SchedulerConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));
        
        info!("Initializing work scheduler with {} max concurrent jobs", config.max_concurrent);
        
        Self {
            memory_monitor,
            queue: Arc::new(Mutex::new(WorkQueue::new())),
            semaphore,
            config,
            stats: Arc::new(Mutex::new(SchedulerStats::default())),
        }
    }

    /// Add a job to the scheduling queue
    pub async fn schedule_job(&self, input_path: PathBuf) -> Result<u64> {
        let file_size = tokio::fs::metadata(&input_path).await
            .map_err(|e| FastResizeError::system(format!("Failed to get file metadata: {}", e)))?
            .len();

        let work_item = WorkItem::new(input_path, file_size, &self.config);
        let job_id = work_item.id;

        // Add to appropriate priority queue
        {
            let mut queue = self.queue.lock().unwrap();
            queue.add_item(work_item);
            
            let mut stats = self.stats.lock().unwrap();
            stats.jobs_queued += 1;
            stats.average_queue_length = queue.total_items as f64;
        }

        debug!("Scheduled job {} for file: {:?}", job_id, &job_id);
        Ok(job_id)
    }

    /// Get the next job to process (blocks until job is available and resources permit)
    pub async fn get_next_job(&self) -> Result<Option<WorkItem>> {
        let max_wait = tokio::time::Duration::from_secs(self.config.max_wait_time);
        let wait_start = Instant::now();

        // Wait for semaphore permit (concurrency limit)
        let permit = tokio::time::timeout(max_wait, self.semaphore.clone().acquire_owned()).await
            .map_err(|_| FastResizeError::system("Timeout waiting for job slot".to_string()))?
            .map_err(|e| FastResizeError::system(format!("Failed to acquire semaphore: {}", e)))?;

        // Check memory pressure and wait if necessary
        self.wait_for_memory_availability().await?;

        // Get next item from queue
        let work_item = {
            let mut queue = self.queue.lock().unwrap();
            let item = queue.get_next_item();
            
            if let Some(ref _item) = item {
                let mut stats = self.stats.lock().unwrap();
                stats.total_wait_time += wait_start.elapsed();
                stats.average_queue_length = queue.total_items as f64;
            }
            
            item
        };

        if let Some(ref item) = work_item {
            debug!("Assigned job {} to worker", item.id);
        }

        // Permit will be automatically dropped when the job completes
        std::mem::forget(permit);

        Ok(work_item)
    }

    /// Mark a job as completed
    pub fn complete_job(&self, job_id: u64, success: bool, processing_time: Duration) {
        let mut stats = self.stats.lock().unwrap();
        
        if success {
            stats.jobs_completed += 1;
        } else {
            stats.jobs_failed += 1;
        }
        
        stats.total_processing_time += processing_time;
        
        // Update throughput calculation
        let total_jobs = stats.jobs_completed + stats.jobs_failed;
        if total_jobs > 0 && !stats.total_processing_time.is_zero() {
            stats.throughput_files_per_second = total_jobs as f64 / stats.total_processing_time.as_secs_f64();
        }

        debug!("Completed job {} (success: {}, time: {:.2}s)", 
               job_id, success, processing_time.as_secs_f64());
    }

    /// Wait for memory availability before starting a job
    async fn wait_for_memory_availability(&self) -> Result<()> {
        const MAX_MEMORY_WAIT_SECONDS: u64 = 30;
        const MEMORY_CHECK_INTERVAL_MS: u64 = 500;

        let start_wait = Instant::now();
        
        loop {
            let memory_usage = self.memory_monitor.usage_percentage();
            
            if memory_usage < self.config.target_memory_usage {
                break; // Memory is available
            }

            // Check timeout
            if start_wait.elapsed().as_secs() > MAX_MEMORY_WAIT_SECONDS {
                return Err(FastResizeError::system(
                    "Timeout waiting for memory availability".to_string()
                ));
            }

            // Record memory pressure event
            {
                let mut stats = self.stats.lock().unwrap();
                stats.memory_pressure_events += 1;
            }

            warn!("Memory pressure detected: {:.1}% usage, waiting...", memory_usage);
            
            // Wait before checking again
            tokio::time::sleep(tokio::time::Duration::from_millis(MEMORY_CHECK_INTERVAL_MS)).await;
        }

        Ok(())
    }

    /// Get current scheduler statistics
    pub fn get_stats(&self) -> SchedulerStats {
        self.stats.lock().unwrap().clone()
    }

    /// Get current queue status
    pub fn get_queue_status(&self) -> QueueStatus {
        let queue = self.queue.lock().unwrap();
        QueueStatus {
            high_priority_count: queue.high_priority.len(),
            normal_priority_count: queue.normal_priority.len(),
            low_priority_count: queue.low_priority.len(),
            total_count: queue.total_items,
        }
    }

    /// Clear all queued jobs
    pub fn clear_queue(&self) -> usize {
        let mut queue = self.queue.lock().unwrap();
        let cleared_count = queue.total_items;
        queue.clear();
        
        info!("Cleared {} jobs from queue", cleared_count);
        cleared_count
    }

    /// Adjust scheduler configuration at runtime
    pub fn update_config(&mut self, new_config: SchedulerConfig) {
        // Update semaphore if max_concurrent changed
        if new_config.max_concurrent != self.config.max_concurrent {
            self.semaphore = Arc::new(Semaphore::new(new_config.max_concurrent));
            info!("Updated max concurrent jobs to {}", new_config.max_concurrent);
        }

        self.config = new_config;
    }
}

impl WorkQueue {
    fn new() -> Self {
        Self {
            high_priority: VecDeque::new(),
            normal_priority: VecDeque::new(),
            low_priority: VecDeque::new(),
            total_items: 0,
        }
    }

    fn add_item(&mut self, item: WorkItem) {
        match item.priority {
            JobPriority::High => self.high_priority.push_back(item),
            JobPriority::Normal => self.normal_priority.push_back(item),
            JobPriority::Low => self.low_priority.push_back(item),
        }
        self.total_items += 1;
    }

    fn get_next_item(&mut self) -> Option<WorkItem> {
        // Process in priority order: High -> Normal -> Low
        let item = self.high_priority.pop_front()
            .or_else(|| self.normal_priority.pop_front())
            .or_else(|| self.low_priority.pop_front());
            
        if item.is_some() {
            self.total_items = self.total_items.saturating_sub(1);
        }
        
        item
    }

    fn clear(&mut self) {
        self.high_priority.clear();
        self.normal_priority.clear();
        self.low_priority.clear();
        self.total_items = 0;
    }
}

impl WorkItem {
    /// Create a new work item with calculated priority and estimates
    pub fn new(input_path: PathBuf, file_size: u64, config: &SchedulerConfig) -> Self {
        let id = Self::generate_id();
        let priority = Self::calculate_priority(file_size, config);
        let estimated_memory = Self::estimate_memory_usage(file_size);
        let estimated_processing_time = Self::estimate_processing_time(file_size);

        Self {
            id,
            input_path,
            estimated_size: file_size,
            priority,
            created_at: Instant::now(),
            estimated_memory,
            estimated_processing_time,
        }
    }

    /// Generate unique job ID
    fn generate_id() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    /// Calculate job priority based on file characteristics
    fn calculate_priority(file_size: u64, config: &SchedulerConfig) -> JobPriority {
        if file_size >= config.large_file_threshold {
            // Large files get higher priority to start early and avoid blocking
            JobPriority::High
        } else if file_size < 1024 * 1024 { // < 1MB
            // Small files get lower priority and can be batched
            JobPriority::Low
        } else {
            // Medium files get normal priority
            JobPriority::Normal
        }
    }

    /// Estimate memory usage for processing this file
    fn estimate_memory_usage(file_size: u64) -> u64 {
        // Rough estimate: assume decompressed image uses 4 bytes per pixel
        // and compressed file represents about 10% of uncompressed size for JPEG
        // This is a conservative estimate
        file_size * 10 * 4 // 40x the file size
    }

    /// Estimate processing time based on file size
    fn estimate_processing_time(file_size: u64) -> Duration {
        // Base processing time estimation (very rough)
        let base_time_ms = match file_size {
            0..=1_000_000 => 100,           // < 1MB: 100ms
            1_000_001..=10_000_000 => 500,  // 1-10MB: 500ms
            10_000_001..=50_000_000 => 2000, // 10-50MB: 2s
            _ => 5000,                       // > 50MB: 5s
        };
        
        Duration::from_millis(base_time_ms)
    }

    /// Check if this job is high priority
    pub fn is_high_priority(&self) -> bool {
        self.priority == JobPriority::High
    }

    /// Get age of this job in the queue
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// Current status of the job queue
#[derive(Debug, Clone)]
pub struct QueueStatus {
    pub high_priority_count: usize,
    pub normal_priority_count: usize,
    pub low_priority_count: usize,
    pub total_count: usize,
}

impl QueueStatus {
    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.total_count == 0
    }

    /// Get queue depth by priority
    pub fn depth_by_priority(&self) -> String {
        format!("H:{} N:{} L:{}", 
                self.high_priority_count,
                self.normal_priority_count, 
                self.low_priority_count)
    }
}

/// Scheduler performance optimizer
pub struct SchedulerOptimizer {
    scheduler: Arc<Mutex<WorkScheduler>>,
    optimization_interval: Duration,
}

impl SchedulerOptimizer {
    /// Create a new scheduler optimizer
    pub fn new(scheduler: Arc<Mutex<WorkScheduler>>) -> Self {
        Self {
            scheduler,
            optimization_interval: Duration::from_secs(60), // Check every minute
        }
    }

    /// Start continuous optimization in the background
    pub async fn start_optimization(&self) {
        let mut interval = tokio::time::interval(self.optimization_interval);
        
        loop {
            interval.tick().await;
            self.optimize().await;
        }
    }

    /// Perform optimization based on current statistics
    async fn optimize(&self) {
        let current_stats = {
            let scheduler = self.scheduler.lock().unwrap();
            scheduler.get_stats()
        };

        // Analyze performance and suggest optimizations
        if current_stats.memory_pressure_events > 10 {
            warn!("High memory pressure detected ({} events), consider reducing concurrent jobs",
                  current_stats.memory_pressure_events);
        }

        if current_stats.throughput_files_per_second < 0.5 {
            warn!("Low throughput detected ({:.2} files/sec), investigating bottlenecks",
                  current_stats.throughput_files_per_second);
        }

        // Auto-adjust configuration based on performance
        self.auto_tune_configuration(&current_stats).await;
    }

    /// Automatically tune configuration parameters
    async fn auto_tune_configuration(&self, stats: &SchedulerStats) {
        // This is a simplified auto-tuning implementation
        // In production, you might want more sophisticated algorithms
        
        if stats.memory_pressure_events > 20 {
            // Too much memory pressure, reduce concurrency
            let mut scheduler = self.scheduler.lock().unwrap();
            let mut new_config = scheduler.config.clone();
            new_config.max_concurrent = (new_config.max_concurrent * 3 / 4).max(1);
            new_config.target_memory_usage = 65.0; // Lower target
            
            info!("Auto-tuning: Reducing max concurrent jobs to {} due to memory pressure",
                  new_config.max_concurrent);
            
            scheduler.update_config(new_config);
        } else if stats.throughput_files_per_second > 2.0 && stats.memory_pressure_events == 0 {
            // Good performance with no memory issues, try increasing concurrency
            let mut scheduler = self.scheduler.lock().unwrap();
            let mut new_config = scheduler.config.clone();
            new_config.max_concurrent = (new_config.max_concurrent * 5 / 4).min(32);
            
            info!("Auto-tuning: Increasing max concurrent jobs to {} due to good performance",
                  new_config.max_concurrent);
            
            scheduler.update_config(new_config);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::processing::memory::MemoryMonitor;

    #[test]
    fn test_work_item_creation() {
        let config = SchedulerConfig::default();
        let path = PathBuf::from("test.jpg");
        
        let small_item = WorkItem::new(path.clone(), 500_000, &config); // 500KB
        assert_eq!(small_item.priority, JobPriority::Low);
        
        let large_item = WorkItem::new(path.clone(), 100_000_000, &config); // 100MB
        assert_eq!(large_item.priority, JobPriority::High);
        
        let medium_item = WorkItem::new(path, 5_000_000, &config); // 5MB
        assert_eq!(medium_item.priority, JobPriority::Normal);
    }

    #[test]
    fn test_work_queue() {
        let mut queue = WorkQueue::new();
        let config = SchedulerConfig::default();
        
        // Add items with different priorities
        let high_item = WorkItem::new(PathBuf::from("high.jpg"), 100_000_000, &config);
        let normal_item = WorkItem::new(PathBuf::from("normal.jpg"), 5_000_000, &config);
        let low_item = WorkItem::new(PathBuf::from("low.jpg"), 500_000, &config);
        
        queue.add_item(low_item);
        queue.add_item(high_item);
        queue.add_item(normal_item);
        
        // Should return high priority first
        let next = queue.get_next_item().unwrap();
        assert_eq!(next.priority, JobPriority::High);
        
        // Then normal priority
        let next = queue.get_next_item().unwrap();
        assert_eq!(next.priority, JobPriority::Normal);
        
        // Finally low priority
        let next = queue.get_next_item().unwrap();
        assert_eq!(next.priority, JobPriority::Low);
        
        // Queue should be empty
        assert!(queue.get_next_item().is_none());
    }

    #[tokio::test]
    async fn test_scheduler_basic() {
        let memory_monitor = Arc::new(MemoryMonitor::new(Some(1000))); // 1GB
        let config = SchedulerConfig {
            max_concurrent: 2,
            ..Default::default()
        };
        
        let scheduler = WorkScheduler::new(memory_monitor, config);
        
        // Create a temporary file for testing
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.jpg");
        tokio::fs::write(&test_file, b"dummy content").await.unwrap();
        
        // Schedule a job
        let job_id = scheduler.schedule_job(test_file).await.unwrap();
        assert!(job_id > 0);
        
        let queue_status = scheduler.get_queue_status();
        assert_eq!(queue_status.total_count, 1);
    }

    #[test]
    fn test_scheduler_config() {
        let config = SchedulerConfig::default();
        assert!(config.max_concurrent > 0);
        assert!(config.target_memory_usage > 0.0 && config.target_memory_usage <= 100.0);
        assert!(config.batch_size > 0);
    }

    #[test]
    fn test_queue_status() {
        let status = QueueStatus {
            high_priority_count: 2,
            normal_priority_count: 5,
            low_priority_count: 3,
            total_count: 10,
        };
        
        assert!(!status.is_empty());
        assert_eq!(status.total_count, 10);
        
        let depth_text = status.depth_by_priority();
        assert!(depth_text.contains("H:2"));
        assert!(depth_text.contains("N:5"));
        assert!(depth_text.contains("L:3"));
    }

    #[test]
    fn test_work_item_estimates() {
        let large_file_size = 50_000_000; // 50MB
        let memory_estimate = WorkItem::estimate_memory_usage(large_file_size);
        assert!(memory_estimate > large_file_size); // Should be much larger than file size
        
        let processing_time = WorkItem::estimate_processing_time(large_file_size);
        assert!(processing_time.as_millis() > 1000); // Should be > 1s for large files
    }
}