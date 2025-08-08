//! Memory management utilities for efficient image processing

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// Memory pool for reusing image buffers
pub struct MemoryPool {
    small_buffers: Arc<Mutex<VecDeque<Vec<u8>>>>,   // < 1MB
    medium_buffers: Arc<Mutex<VecDeque<Vec<u8>>>>,  // 1-10MB
    large_buffers: Arc<Mutex<VecDeque<Vec<u8>>>>,   // > 10MB
    stats: Arc<Mutex<PoolStats>>,
}

#[derive(Debug, Default)]
pub struct PoolStats {
    pub small_allocated: usize,
    pub small_reused: usize,
    pub medium_allocated: usize,
    pub medium_reused: usize,
    pub large_allocated: usize,
    pub large_reused: usize,
    pub total_memory_saved: u64,
}

impl MemoryPool {
    /// Create a new memory pool
    pub fn new() -> Self {
        Self {
            small_buffers: Arc::new(Mutex::new(VecDeque::with_capacity(50))),
            medium_buffers: Arc::new(Mutex::new(VecDeque::with_capacity(20))),
            large_buffers: Arc::new(Mutex::new(VecDeque::with_capacity(5))),
            stats: Arc::new(Mutex::new(PoolStats::default())),
        }
    }

    /// Acquire a buffer of the specified minimum size
    pub fn acquire_buffer(&self, min_size: usize) -> ManagedBuffer {
        let buffer = if min_size < 1024 * 1024 {
            // Small buffer (< 1MB)
            self.acquire_from_pool(&self.small_buffers, min_size, BufferSize::Small)
        } else if min_size < 10 * 1024 * 1024 {
            // Medium buffer (1-10MB)
            self.acquire_from_pool(&self.medium_buffers, min_size, BufferSize::Medium)
        } else {
            // Large buffer (> 10MB)
            self.acquire_from_pool(&self.large_buffers, min_size, BufferSize::Large)
        };

        ManagedBuffer {
            buffer,
            pool: Arc::clone(&self.stats),
            size_category: if min_size < 1024 * 1024 {
                BufferSize::Small
            } else if min_size < 10 * 1024 * 1024 {
                BufferSize::Medium
            } else {
                BufferSize::Large
            },
            return_to_pool: match min_size {
                size if size < 1024 * 1024 => Arc::clone(&self.small_buffers),
                size if size < 10 * 1024 * 1024 => Arc::clone(&self.medium_buffers),
                _ => Arc::clone(&self.large_buffers),
            },
        }
    }

    fn acquire_from_pool(
        &self,
        pool: &Arc<Mutex<VecDeque<Vec<u8>>>>,
        min_size: usize,
        size_category: BufferSize,
    ) -> Vec<u8> {
        let mut pool_guard = pool.lock().unwrap();
        
        // Try to find a suitable buffer in the pool
        for _ in 0..pool_guard.len() {
            if let Some(mut buffer) = pool_guard.pop_front() {
                if buffer.capacity() >= min_size {
                    buffer.clear();
                    buffer.resize(min_size, 0);
                    
                    // Update statistics
                    let mut stats = self.stats.lock().unwrap();
                    match size_category {
                        BufferSize::Small => {
                            stats.small_reused += 1;
                            stats.total_memory_saved += min_size as u64;
                        }
                        BufferSize::Medium => {
                            stats.medium_reused += 1;
                            stats.total_memory_saved += min_size as u64;
                        }
                        BufferSize::Large => {
                            stats.large_reused += 1;
                            stats.total_memory_saved += min_size as u64;
                        }
                    }
                    
                    debug!("Reused buffer: {} bytes ({:?})", min_size, size_category);
                    return buffer;
                } else {
                    // Buffer too small, put it back
                    pool_guard.push_back(buffer);
                    break;
                }
            }
        }

        // No suitable buffer found, allocate new one
        let buffer = vec![0; min_size];
        
        // Update statistics
        let mut stats = self.stats.lock().unwrap();
        match size_category {
            BufferSize::Small => stats.small_allocated += 1,
            BufferSize::Medium => stats.medium_allocated += 1,
            BufferSize::Large => stats.large_allocated += 1,
        }
        
        debug!("Allocated new buffer: {} bytes ({:?})", min_size, size_category);
        buffer
    }

    /// Get memory pool statistics
    pub fn stats(&self) -> PoolStats {
        self.stats.lock().unwrap().clone()
    }

    /// Clear all cached buffers (useful for memory cleanup)
    pub fn clear(&self) {
        self.small_buffers.lock().unwrap().clear();
        self.medium_buffers.lock().unwrap().clear();
        self.large_buffers.lock().unwrap().clear();
        debug!("Memory pool cleared");
    }

    /// Get current memory usage of the pool
    pub fn current_memory_usage(&self) -> usize {
        let small_usage: usize = self.small_buffers.lock().unwrap()
            .iter().map(|b| b.capacity()).sum();
        let medium_usage: usize = self.medium_buffers.lock().unwrap()
            .iter().map(|b| b.capacity()).sum();
        let large_usage: usize = self.large_buffers.lock().unwrap()
            .iter().map(|b| b.capacity()).sum();

        small_usage + medium_usage + large_usage
    }
}

impl Default for MemoryPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PoolStats {
    fn clone(&self) -> Self {
        Self {
            small_allocated: self.small_allocated,
            small_reused: self.small_reused,
            medium_allocated: self.medium_allocated,
            medium_reused: self.medium_reused,
            large_allocated: self.large_allocated,
            large_reused: self.large_reused,
            total_memory_saved: self.total_memory_saved,
        }
    }
}

/// A buffer that automatically returns to the pool when dropped
pub struct ManagedBuffer {
    buffer: Vec<u8>,
    pool: Arc<Mutex<PoolStats>>,
    size_category: BufferSize,
    return_to_pool: Arc<Mutex<VecDeque<Vec<u8>>>>,
}

#[derive(Debug, Clone, Copy)]
enum BufferSize {
    Small,
    Medium,
    Large,
}

impl ManagedBuffer {
    /// Get a reference to the buffer
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    /// Get a mutable reference to the buffer
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    /// Get the buffer size
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Resize the buffer
    pub fn resize(&mut self, new_len: usize, value: u8) {
        self.buffer.resize(new_len, value);
    }

    /// Get the buffer capacity
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }
}

impl Drop for ManagedBuffer {
    fn drop(&mut self) {
        // Return buffer to pool if it's reasonable size
        const MAX_POOLED_SIZE: usize = 100 * 1024 * 1024; // 100MB
        
        if self.buffer.capacity() <= MAX_POOLED_SIZE {
            let mut pool = self.return_to_pool.lock().unwrap();
            
            // Limit pool size to prevent unlimited memory growth
            let max_pool_size: usize = match self.size_category {
                BufferSize::Small => 100,
                BufferSize::Medium => 50,
                BufferSize::Large => 20,
            };
            
            if pool.len() < max_pool_size {
                pool.push_back(std::mem::take(&mut self.buffer));
                debug!("Buffer returned to pool ({:?})", self.size_category);
            } else {
                debug!("Pool full, buffer discarded ({:?})", self.size_category);
            }
        } else {
            debug!("Buffer too large for pool, discarded: {} bytes", self.buffer.capacity());
        }
    }
}

impl AsRef<[u8]> for ManagedBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.buffer
    }
}

impl AsMut<[u8]> for ManagedBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }
}

/// Memory usage monitor for the system
pub struct MemoryMonitor {
    max_memory_usage: u64,
    current_usage_estimate: Arc<Mutex<u64>>,
}

impl MemoryMonitor {
    /// Create a new memory monitor with the specified limit
    pub fn new(max_memory_mb: Option<u64>) -> Self {
        let max_memory_usage = max_memory_mb
            .unwrap_or_else(|| {
                // Auto-detect available memory and use 75% of it
                use sysinfo::{System, SystemExt};
                let mut system = System::new();
                system.refresh_memory();
                let available_mb = system.available_memory() / 1024 / 1024;
                (available_mb * 75 / 100).max(512) // At least 512MB
            }) * 1024 * 1024; // Convert to bytes

        Self {
            max_memory_usage,
            current_usage_estimate: Arc::new(Mutex::new(0)),
        }
    }

    /// Check if we can allocate the specified amount of memory
    pub fn can_allocate(&self, size: u64) -> bool {
        let current = *self.current_usage_estimate.lock().unwrap();
        current + size <= self.max_memory_usage
    }

    /// Record memory allocation
    pub fn allocate(&self, size: u64) {
        let mut current = self.current_usage_estimate.lock().unwrap();
        *current += size;
        
        if *current > self.max_memory_usage {
            warn!("Memory usage exceeded limit: {:.2}MB / {:.2}MB",
                 *current as f64 / 1024.0 / 1024.0,
                 self.max_memory_usage as f64 / 1024.0 / 1024.0);
        }
    }

    /// Record memory deallocation
    pub fn deallocate(&self, size: u64) {
        let mut current = self.current_usage_estimate.lock().unwrap();
        *current = current.saturating_sub(size);
    }

    /// Get current memory usage estimate
    pub fn current_usage(&self) -> u64 {
        *self.current_usage_estimate.lock().unwrap()
    }

    /// Get maximum allowed memory usage
    pub fn max_usage(&self) -> u64 {
        self.max_memory_usage
    }

    /// Get memory usage percentage
    pub fn usage_percentage(&self) -> f64 {
        let current = self.current_usage() as f64;
        let max = self.max_memory_usage as f64;
        (current / max * 100.0).min(100.0)
    }

    /// Check if we're approaching memory limits
    pub fn is_memory_pressure(&self) -> bool {
        self.usage_percentage() > 80.0
    }
}

/// RAII wrapper for tracking memory allocation/deallocation
pub struct MemoryTracker {
    monitor: Arc<MemoryMonitor>,
    allocated_size: u64,
}

impl MemoryTracker {
    /// Create a new memory tracker
    pub fn new(monitor: Arc<MemoryMonitor>, size: u64) -> Option<Self> {
        if monitor.can_allocate(size) {
            monitor.allocate(size);
            Some(Self {
                monitor,
                allocated_size: size,
            })
        } else {
            None
        }
    }
}

impl Drop for MemoryTracker {
    fn drop(&mut self) {
        self.monitor.deallocate(self.allocated_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pool_basic() {
        let pool = MemoryPool::new();
        
        // Acquire and release a buffer
        {
            let buffer = pool.acquire_buffer(1024);
            assert_eq!(buffer.len(), 1024);
        }
        
        // Should have reused the buffer
        {
            let buffer = pool.acquire_buffer(512);
            assert!(buffer.len() >= 512);
        }
        
        let stats = pool.stats();
        assert!(stats.small_allocated > 0 || stats.small_reused > 0);
    }

    #[test]
    fn test_buffer_size_categories() {
        let pool = MemoryPool::new();
        
        let small = pool.acquire_buffer(1024);        // < 1MB
        let medium = pool.acquire_buffer(5 * 1024 * 1024); // 5MB
        let large = pool.acquire_buffer(50 * 1024 * 1024);  // 50MB
        
        assert_eq!(small.len(), 1024);
        assert_eq!(medium.len(), 5 * 1024 * 1024);
        assert_eq!(large.len(), 50 * 1024 * 1024);
    }

    #[test]
    fn test_memory_monitor() {
        let monitor = MemoryMonitor::new(Some(100)); // 100MB limit
        
        assert!(monitor.can_allocate(50 * 1024 * 1024)); // 50MB
        assert!(!monitor.can_allocate(150 * 1024 * 1024)); // 150MB
        
        monitor.allocate(50 * 1024 * 1024);
        assert_eq!(monitor.usage_percentage(), 50.0);
        
        monitor.deallocate(25 * 1024 * 1024);
        assert_eq!(monitor.usage_percentage(), 25.0);
    }

    #[test]
    fn test_memory_tracker() {
        let monitor = Arc::new(MemoryMonitor::new(Some(100))); // 100MB
        let size = 50 * 1024 * 1024; // 50MB
        
        {
            let _tracker = MemoryTracker::new(monitor.clone(), size).unwrap();
            assert_eq!(monitor.current_usage(), size);
        }
        
        // Should be deallocated when tracker is dropped
        assert_eq!(monitor.current_usage(), 0);
    }

    #[test]
    fn test_managed_buffer_operations() {
        let pool = MemoryPool::new();
        let mut buffer = pool.acquire_buffer(1024);
        
        assert_eq!(buffer.len(), 1024);
        assert!(!buffer.is_empty());
        
        buffer.resize(2048, 255);
        assert_eq!(buffer.len(), 2048);
        
        let slice = buffer.as_slice();
        assert_eq!(slice.len(), 2048);
        
        let mut_slice = buffer.as_mut_slice();
        mut_slice[0] = 100;
        assert_eq!(buffer.as_slice()[0], 100);
    }

    #[test]
    fn test_pool_statistics() {
        let pool = MemoryPool::new();
        
        // Allocate some buffers
        let _buffer1 = pool.acquire_buffer(1024);
        let _buffer2 = pool.acquire_buffer(2048);
        
        let stats = pool.stats();
        assert!(stats.small_allocated >= 2);
        
        // Drop one and reacquire
        drop(_buffer1);
        let _buffer3 = pool.acquire_buffer(1024);
        
        let stats = pool.stats();
        assert!(stats.small_reused >= 1);
        assert!(stats.total_memory_saved >= 1024);
    }
}