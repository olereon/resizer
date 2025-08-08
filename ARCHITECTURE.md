# FastResize Architecture

This document describes the high-level architecture, design decisions, and technical implementation of FastResize.

## Overview

FastResize is a high-performance, memory-efficient batch image resizer built in Rust. It's designed to process thousands of large images efficiently while maintaining a simple CLI interface for automation workflows.

## Design Goals

### Primary Goals
1. **Performance**: 3-5x faster than Python alternatives, approaching C++ performance
2. **Memory Efficiency**: Handle large batches without excessive memory usage
3. **Reliability**: Graceful error handling, continue processing on individual failures
4. **Automation**: CLI-first design for scripting and CI/CD integration

### Secondary Goals  
1. **Cross-platform**: Single binary that works on Linux, macOS, Windows
2. **Easy Distribution**: No runtime dependencies, statically linked binary
3. **Extensibility**: Plugin architecture for custom processing pipelines
4. **Monitoring**: Built-in metrics and observability

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          CLI Interface                          │
│                     (Clap + Configuration)                     │
└─────────────────────┬───────────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────────┐
│                     Orchestrator                               │
│              (Job Planning & Coordination)                     │
└─────────────┬───────────────────────────┬───────────────────────┘
              │                           │
┌─────────────▼───────────┐    ┌─────────▼─────────────────────────┐
│    File Discovery       │    │       Processing Engine           │
│   (Walking, Filtering)  │    │    (Parallel + Memory Pool)       │
└─────────────┬───────────┘    └─────────┬─────────────────────────┘
              │                          │
              │        ┌─────────────────▼─────────────────────────┐
              │        │              Worker Threads               │
              │        │         (Image Processing Core)           │
              │        └─────────┬─────────────┬─────────────────────┘
              │                  │             │
┌─────────────▼─────────┐ ┌─────▼─────┐ ┌─────▼─────────────────────┐
│    Progress Tracker   │ │  I/O Pool │ │      Output Manager       │
│  (Real-time Updates)  │ │(Async I/O)│ │   (File Writing + QA)     │
└───────────────────────┘ └───────────┘ └───────────────────────────┘
```

## Core Components

### 1. CLI Interface (`src/main.rs`)

**Responsibilities:**
- Command-line argument parsing
- Configuration file loading
- Environment variable handling
- Input validation and early error detection

**Technology:**
- `clap` for robust argument parsing with derive macros
- `serde` for configuration deserialization
- `toml` and `serde_yaml` for config file formats

**Design Patterns:**
```rust
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[arg(short, long, help = "Input directory or file")]
    input: PathBuf,
    
    #[arg(short, long, help = "Output directory")]
    output: PathBuf,
    
    #[command(flatten)]
    resize_options: ResizeOptions,
    
    #[command(flatten)]  
    processing_options: ProcessingOptions,
}
```

### 2. Configuration Management (`src/config/`)

**Responsibilities:**
- Configuration file parsing (YAML/TOML)
- Processing profiles management
- Environment variable integration
- Configuration validation

**Architecture:**
```rust
pub struct Config {
    pub profiles: HashMap<String, ProcessingProfile>,
    pub processing: ProcessingConfig,
    pub automation: AutomationConfig,
}

pub struct ProcessingProfile {
    pub resize_mode: ResizeMode,
    pub quality: u8,
    pub format: Option<ImageFormat>,
    pub naming: NamingConfig,
}
```

### 3. File Discovery (`src/discovery.rs`)

**Responsibilities:**
- Recursive directory traversal
- File format detection and filtering
- Size and permission validation
- Batch size optimization

**Performance Optimizations:**
- Parallel directory walking with `walkdir`
- Early filtering to avoid processing unsuitable files
- Memory-mapped file header reading for format detection

### 4. Processing Engine (`src/processing/`)

**Core Architecture:**
```rust
pub struct ProcessingEngine {
    thread_pool: rayon::ThreadPool,
    memory_pool: MemoryPool,
    progress_tracker: Arc<ProgressTracker>,
    error_handler: ErrorHandler,
}

impl ProcessingEngine {
    pub async fn process_batch(&self, files: Vec<InputFile>) -> Result<BatchResult> {
        files.par_iter()
            .map(|file| self.process_single_file(file))
            .collect()
    }
}
```

#### 4.1 Memory Management (`src/processing/memory.rs`)

**Key Features:**
- **Buffer Pooling**: Pre-allocated buffers reused across operations
- **Memory-Mapped Files**: For files >100MB to avoid loading into memory
- **Streaming Processing**: Process images in chunks for very large files
- **Automatic Cleanup**: RAII for resource management

**Implementation:**
```rust
pub struct MemoryPool {
    small_buffers: Arc<Mutex<Vec<Vec<u8>>>>,    // <1MB
    medium_buffers: Arc<Mutex<Vec<Vec<u8>>>>,   // 1-10MB  
    large_buffers: Arc<Mutex<Vec<Vec<u8>>>>,    // >10MB
}

pub struct StreamingProcessor {
    tile_size: usize,          // Default: 2048x2048
    overlap: usize,            // For filtering operations
    memory_limit: usize,       // Max concurrent memory usage
}
```

#### 4.2 Image Processing Core (`src/processing/resize.rs`)

**Resize Algorithms:**
- **Scale Factor**: Multiply dimensions by factor
- **Dimension-based**: Width or height constrained, maintain aspect ratio
- **Fit/Fill**: Advanced constraint modes for specific use cases

**Optimization Techniques:**
- **SIMD Instructions**: Vectorized operations using `wide` crate
- **Multi-threaded Tiles**: Split large images across CPU cores  
- **Quality-aware**: Different algorithms based on quality settings
- **Format-specific**: Optimized paths for JPEG, PNG, WebP

```rust
pub enum ResizeMode {
    Scale { factor: f32 },
    Width { width: u32 },
    Height { height: u32 },
    Fit { width: u32, height: u32 },
    Fill { width: u32, height: u32 },
}

pub struct ResizeConfig {
    pub mode: ResizeMode,
    pub filter: FilterType,
    pub quality: u8,
    pub preserve_aspect: bool,
}
```

#### 4.3 Format Handling (`src/processing/formats.rs`)

**Supported Formats:**
- **Input**: JPEG, PNG, WebP, GIF, TIFF, BMP, AVIF, HEIC
- **Output**: JPEG, PNG, WebP (with format conversion support)

**Format-Specific Optimizations:**
- **JPEG**: libjpeg-turbo integration, progressive encoding
- **PNG**: Optimal compression level selection
- **WebP**: VP8/VP8L encoding based on content analysis

### 5. Parallel Processing (`src/parallel/`)

**Architecture:**
- **Work-Stealing**: Rayon's work-stealing thread pool
- **Back-pressure**: Limit concurrent operations based on memory usage
- **Progress Coordination**: Lock-free progress updates

**Thread Pool Configuration:**
```rust
pub struct ParallelConfig {
    pub num_threads: Option<usize>,      // Default: CPU cores
    pub max_concurrent_files: usize,     // Memory-based limit
    pub batch_size: usize,               // Files per batch
    pub stack_size: usize,               // Thread stack size
}
```

**Memory-aware Scheduling:**
```rust
pub struct Scheduler {
    memory_monitor: MemoryMonitor,
    active_jobs: AtomicUsize,
    pending_queue: crossbeam::queue::SegQueue<Job>,
}

impl Scheduler {
    fn should_start_job(&self) -> bool {
        self.memory_monitor.available_memory() > MIN_MEMORY_THRESHOLD
            && self.active_jobs.load(Ordering::Relaxed) < self.max_concurrent
    }
}
```

### 6. Progress Tracking (`src/parallel/progress.rs`)

**Features:**
- Real-time progress updates
- ETA calculation
- Per-file status tracking
- Error aggregation

**Implementation:**
```rust
pub struct ProgressTracker {
    total_files: AtomicUsize,
    completed_files: AtomicUsize,
    failed_files: AtomicUsize,
    current_file: Arc<RwLock<Option<String>>>,
    start_time: Instant,
    reporter: Box<dyn ProgressReporter + Send + Sync>,
}

pub trait ProgressReporter {
    fn update(&self, progress: &ProgressState);
    fn error(&self, file: &str, error: &ProcessingError);
    fn completed(&self, summary: &BatchSummary);
}
```

### 7. Error Handling (`src/error.rs`)

**Error Categories:**
```rust
#[derive(Debug, thiserror::Error)]
pub enum ProcessingError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Image format not supported: {format}")]
    UnsupportedFormat { format: String },
    
    #[error("Image too large: {width}x{height} (limit: {limit}")]
    ImageTooLarge { width: u32, height: u32, limit: usize },
    
    #[error("Memory allocation failed")]
    OutOfMemory,
    
    #[error("Processing timeout after {timeout}s")]
    Timeout { timeout: u64 },
}
```

**Error Recovery Strategy:**
1. **Validation Errors**: Fail fast before processing begins
2. **Individual File Errors**: Log error, continue with remaining files
3. **System Errors**: Attempt recovery, fallback to safe mode
4. **Critical Errors**: Clean shutdown with detailed error reporting

### 8. Automation Features (`src/automation/`)

#### 8.1 File System Watching (`src/automation/watch.rs`)

**Implementation:**
- `notify` crate for cross-platform file watching
- Debouncing to handle file system events efficiently
- Event filtering to process only relevant file changes

```rust
pub struct FileWatcher {
    watcher: RecommendedWatcher,
    event_handler: Box<dyn EventHandler + Send + Sync>,
    debouncer: Debouncer,
}

pub trait EventHandler {
    fn handle_new_file(&self, path: &Path) -> Result<()>;
    fn handle_modified_file(&self, path: &Path) -> Result<()>;
    fn handle_deleted_file(&self, path: &Path) -> Result<()>;
}
```

#### 8.2 Batch Job Processing (`src/automation/batch.rs`)

**Features:**
- Job queue management
- Priority-based processing
- Job persistence and recovery
- Retry mechanisms

## Performance Characteristics

### Memory Usage Patterns

**Memory Efficiency Strategies:**
1. **Streaming Processing**: Never load entire large images into memory
2. **Buffer Reuse**: Pool buffers to minimize allocations
3. **Memory Mapping**: Use OS virtual memory for very large files
4. **Garbage Collection**: Proactive cleanup of temporary resources

**Memory Limits:**
```rust
pub struct MemoryConfig {
    pub max_image_memory: usize,        // Default: 1GB
    pub max_total_memory: usize,        // Default: 4GB  
    pub buffer_pool_size: usize,        // Default: 100MB
    pub enable_memory_mapping: bool,    // Default: true for >100MB files
}
```

### CPU Utilization

**Thread Management:**
- **Work Stealing**: Rayon automatically balances load across cores
- **NUMA Awareness**: Thread affinity for better cache locality
- **Hyperthreading**: Optimize for physical cores vs logical cores

**SIMD Optimization:**
```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

fn resize_simd_avx2(src: &[u8], dst: &mut [u8], width: usize) {
    // Vectorized image processing using AVX2 instructions
    unsafe {
        // Process 32 bytes (8 pixels) at once
        for chunk in src.chunks_exact(32) {
            let pixels = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);
            // ... SIMD operations
        }
    }
}
```

### I/O Performance

**Async I/O Strategy:**
- **Dedicated I/O Threads**: Separate thread pool for file operations
- **Read-ahead**: Predictive loading of upcoming files
- **Write Batching**: Combine small writes for better throughput

**Disk Optimization:**
```rust
pub struct IoConfig {
    pub read_buffer_size: usize,        // Default: 64KB
    pub write_buffer_size: usize,       // Default: 64KB
    pub enable_direct_io: bool,         // Bypass OS cache for large files
    pub io_thread_count: usize,         // Default: 2-4 threads
}
```

## Scalability Considerations

### Horizontal Scaling
- **Process-based Parallelism**: Multiple FastResize instances
- **Work Distribution**: Split large jobs across multiple machines
- **Shared Storage**: Network file systems for distributed processing

### Vertical Scaling  
- **Memory Scaling**: Efficient use of available RAM
- **CPU Scaling**: Linear performance scaling with core count
- **I/O Scaling**: Parallel disk access patterns

### Cloud Deployment
- **Container Support**: Docker images with optimized base layers
- **Auto-scaling**: CPU/memory-based scaling triggers
- **Storage Integration**: Support for cloud storage APIs

## Security Considerations

### Input Validation
- **File Format Verification**: Validate magic bytes, not just extensions
- **Size Limits**: Configurable limits to prevent resource exhaustion
- **Path Validation**: Prevent directory traversal attacks

### Memory Safety
- **Rust Ownership**: Compile-time memory safety guarantees
- **Bounds Checking**: Safe array access throughout codebase
- **Resource Limits**: Configurable memory and CPU limits

### Sandboxing
- **Process Isolation**: Run worker processes with limited privileges
- **File System Restrictions**: Restrict access to input/output directories only
- **Network Isolation**: No network access required for core functionality

## Monitoring and Observability

### Metrics Collection
```rust
pub struct Metrics {
    pub files_processed: Counter,
    pub processing_time: Histogram,
    pub memory_usage: Gauge,
    pub error_rate: Counter,
}
```

### Logging Strategy
- **Structured Logging**: JSON format for machine parsing
- **Log Levels**: Configurable verbosity levels
- **Performance Logging**: Processing time and resource usage

### Health Checks
- **Readiness**: Can accept new work
- **Liveness**: Process is responsive
- **Resource Usage**: Memory and CPU within limits

## Future Architecture Considerations

### Planned Enhancements
1. **GPU Acceleration**: CUDA/OpenCL for supported operations
2. **WebAssembly**: Browser-compatible processing engine
3. **Distributed Processing**: Multi-node job distribution
4. **Plugin Architecture**: Custom processing pipelines

### Architectural Evolution
1. **Microservices**: Break into smaller, composable services
2. **Event Streaming**: Kafka/Redis for job queuing
3. **API Gateway**: REST API for programmatic access
4. **Database Integration**: Job history and analytics

---

This architecture provides the foundation for a high-performance, scalable image processing system while maintaining simplicity and reliability for automation workflows.