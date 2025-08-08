# FastResize - Claude Development Guide

This file contains development instructions and context for Claude AI when working on the FastResize project.

## Project Context

FastResize is a high-performance batch image resizer written in Rust, designed to replace a JavaScript/React-based web implementation for automation workflows requiring processing of large volumes of high-resolution images.

### Key Requirements
- **Performance**: Handle large images (>100MB, >10000x10000 pixels) efficiently
- **Automation**: CLI-first design for scripting and CI/CD integration
- **Memory Efficiency**: Process thousands of files without memory leaks
- **Reliability**: Graceful error handling and recovery

## Technology Decisions

### Why Rust?
- **Performance**: 3-5x faster than Python, matches C++ with better safety
- **Memory Safety**: Ownership system prevents leaks in long-running operations
- **Concurrency**: Rayon enables fearless parallelism for batch processing
- **Single Binary**: No runtime dependencies, easy deployment
- **Ecosystem**: Mature image processing crates (`image`, `imageproc`)

### Architecture Principles
1. **Memory Efficiency**: Use streaming/tiling for large images, memory pools for buffers
2. **Parallel Processing**: Work-stealing queues, CPU core utilization
3. **Error Recovery**: Continue processing on individual failures
4. **Progressive Enhancement**: Core → Performance → Automation features

## Development Guidelines

### Code Organization
```
src/
├── main.rs           # CLI entry point, argument parsing
├── lib.rs            # Library interface
├── config/
│   ├── mod.rs        # Configuration management
│   └── profiles.rs   # Processing profiles
├── processing/
│   ├── mod.rs        # Core processing logic
│   ├── resize.rs     # Resize algorithms
│   ├── formats.rs    # Format-specific handling
│   └── memory.rs     # Memory management utilities
├── parallel/
│   ├── mod.rs        # Parallel processing coordination
│   ├── worker.rs     # Worker thread implementation
│   └── progress.rs   # Progress tracking
└── automation/
    ├── mod.rs        # Automation features
    ├── watch.rs      # File system watching
    └── batch.rs      # Batch job processing
```

### Performance Standards
- **Memory Usage**: <2GB for 1000 4K images
- **Processing Speed**: <200ms per 4K image on modern CPU
- **Concurrency**: Utilize all available CPU cores
- **Large Files**: Handle >100MB images without loading entirely into memory

### Dependencies Strategy
```toml
[dependencies]
# Core image processing - mature, well-maintained
image = "0.24"
imageproc = "0.23"

# Parallel processing - industry standard
rayon = "1.7"

# CLI and configuration - ergonomic, feature-complete
clap = { version = "4.4", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
serde_yaml = "0.9"

# Async I/O and file watching - tokio ecosystem
tokio = { version = "1.32", features = ["full"] }
notify = "6.0"

# Progress and logging - user experience
indicatif = "0.17"
tracing = "0.1"
tracing-subscriber = "0.3"

# Memory management - performance critical
memmap2 = "0.7"
```

### Error Handling Philosophy
1. **Fail Fast**: Validate inputs before processing begins
2. **Graceful Degradation**: Continue batch processing on individual failures
3. **Detailed Context**: Provide file-specific error information
4. **Recovery**: Implement retry mechanisms for transient failures

### Testing Strategy
```rust
#[cfg(test)]
mod tests {
    // Unit tests for core algorithms
    #[test]
    fn test_resize_scale_factor() { }
    
    #[test] 
    fn test_memory_efficiency() { }
    
    // Integration tests with real images
    #[test]
    fn test_batch_processing() { }
    
    // Performance regression tests
    #[test]
    fn benchmark_processing_speed() { }
}
```

## Implementation Phases

### Phase 1: Core Functionality (MVP)
**Goal**: Working CLI with basic batch resizing
- [ ] Project setup with Cargo.toml
- [ ] CLI argument parsing with clap
- [ ] Basic image loading and resizing
- [ ] File I/O with error handling
- [ ] Simple parallel processing with rayon
- [ ] Progress reporting

### Phase 2: Performance Optimization
**Goal**: Handle large files efficiently
- [ ] Memory pooling for buffer reuse
- [ ] Streaming/tiling for large images
- [ ] SIMD optimizations where applicable
- [ ] Memory-mapped file processing
- [ ] Advanced error recovery

### Phase 3: Automation Features
**Goal**: Production-ready automation
- [ ] Configuration file support (YAML/TOML)
- [ ] Processing profiles
- [ ] Watch mode for folder monitoring
- [ ] Batch job processing
- [ ] JSON output for integration

### Phase 4: Polish and Distribution
**Goal**: Production deployment
- [ ] Cross-platform binary builds
- [ ] Performance benchmarking
- [ ] Documentation and examples
- [ ] CI/CD pipeline
- [ ] Package manager integration

## Code Quality Standards

### Rust Best Practices
- Use `clippy` lints: `#![warn(clippy::all, clippy::pedantic)]`
- Format with `rustfmt`
- Document public APIs with `///` comments
- Use `tracing` instead of `println!` for logging
- Handle all `Result` types explicitly

### Performance Guidelines
- Profile with `cargo bench` and `perf`
- Minimize allocations in hot paths
- Use `Arc` and `Rc` judiciously
- Prefer zero-copy operations
- Benchmark against competitors regularly

### Memory Safety
- Avoid `unsafe` unless absolutely necessary
- Use `Rc/Arc` for shared ownership
- Leverage Rust's ownership system
- Test with Valgrind/AddressSanitizer
- Monitor memory usage in long-running processes

## CLI Design Principles

### User Experience
- **Sane Defaults**: Works without configuration
- **Progressive Disclosure**: Basic → Advanced options
- **Clear Feedback**: Progress bars, error messages
- **Scriptable**: Reliable exit codes, JSON output

### Automation-Friendly
```bash
# Should be easy to script
fastresize --input "$INPUT" --output "$OUTPUT" --config "$CONFIG" --json

# Reliable exit codes
if fastresize --dry-run --input photos/; then
    fastresize --input photos/ --output web/
fi
```

### Configuration Hierarchy
1. Command-line arguments (highest priority)
2. Environment variables (FASTRESIZE_*)
3. Configuration file
4. Built-in defaults (lowest priority)

## Integration Points

### CI/CD Integration
- Exit codes: 0 (success), 1 (partial failure), 2 (complete failure)
- JSON output for machine parsing
- Docker container support
- GitHub Actions integration examples

### Monitoring and Observability
- Structured logging with `tracing`
- Metrics collection (processing time, memory usage)
- Health checks for long-running watch mode
- Performance profiling hooks

## Testing Data

### Sample Images for Testing
- **Small**: 100x100 - 1000x1000 (various formats)
- **Medium**: 2000x2000 - 4000x4000 (typical photos)  
- **Large**: 8000x8000+ (high-resolution scans)
- **Huge**: >50MB files (stress testing)
- **Edge Cases**: Corrupted files, unusual dimensions

### Performance Benchmarks
- Compare against ImageMagick, Sharp, Pillow
- Memory usage profiling
- Processing time measurements
- Scalability testing (1 → 10000 files)

## Deployment Considerations

### Binary Distribution
- Cross-compile for major platforms (Linux, macOS, Windows)
- Static linking for minimal dependencies
- Size optimization with `strip` and compression
- Digital signatures for security

### Package Managers
- Cargo crates.io publication
- Homebrew formula (macOS)
- APT/YUM packages (Linux)
- Chocolatey package (Windows)
- Docker Hub images

## Common Pitfalls to Avoid

1. **Memory Leaks**: Always clean up image buffers
2. **Thread Starvation**: Don't create unlimited threads
3. **File Handle Limits**: Close files promptly
4. **Large Image OOM**: Use streaming for huge files
5. **Platform Differences**: Test path handling across OS
6. **Integer Overflow**: Validate dimensions and calculations
7. **Format-Specific Issues**: Handle JPEG/PNG differences

## Development Commands

```bash
# Development workflow
cargo watch -x "test --lib"
cargo watch -x "run -- --help"

# Performance testing
cargo bench
cargo test --release

# Memory profiling
valgrind target/release/fastresize --input test/ --output out/

# Cross-compilation
cargo build --target x86_64-pc-windows-gnu
cargo build --target x86_64-apple-darwin
```

## External Resources

### Rust Image Processing
- [image-rs documentation](https://docs.rs/image/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Rayon documentation](https://docs.rs/rayon/)

### Image Processing Theory
- [SIMD image processing techniques](https://fgiesen.wordpress.com/2013/11/04/bitmaps-and-linear-algebra/)
- [Memory-efficient image algorithms](https://stackoverflow.com/questions/tagged/image-processing+memory)
- [Color space handling](https://ninedegreesbelow.com/photography/icc-rgb-color-spaces.html)

### Performance References
- [ImageMagick benchmarks](https://imagemagick.org/script/architecture.php#performance)
- [libvips performance comparisons](https://github.com/libvips/libvips/wiki/Speed-and-memory-use)

---

## Notes for Future Claude Sessions

When working on this project:
1. Always prioritize performance and memory efficiency
2. Test with real-world large images
3. Maintain CLI compatibility across versions
4. Document performance implications of changes
5. Consider automation use cases in design decisions