# Contributing to FastResize

Thank you for your interest in contributing to FastResize! This document provides guidelines and information for contributors.

## Code of Conduct

This project adheres to a code of conduct adapted from the [Contributor Covenant](https://www.contributor-covenant.org/). By participating, you are expected to uphold this code.

### Our Standards

- Using welcoming and inclusive language
- Being respectful of differing viewpoints and experiences  
- Gracefully accepting constructive criticism
- Focusing on what is best for the community
- Showing empathy towards other community members

## Getting Started

### Prerequisites

- **Rust**: Install via [rustup](https://rustup.rs/) (minimum version 1.70)
- **Git**: For version control
- **Test Images**: Download sample images for testing

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone the repository  
git clone https://github.com/olereon/resizer.git
cd resizer

# Install development tools
cargo install cargo-watch cargo-nextest cargo-audit
```

### Development Setup

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run with file watching during development
cargo watch -x test
cargo watch -x "run -- --help"

# Format code
cargo fmt

# Run linting
cargo clippy
```

## How to Contribute

### Reporting Issues

Before creating an issue:
1. Search existing issues to avoid duplicates
2. Use the latest version of FastResize
3. Include system information and error details

**Bug Report Template:**
```markdown
## Bug Description
Brief description of the bug

## Steps to Reproduce
1. Step one
2. Step two
3. Step three

## Expected Behavior
What should have happened

## Actual Behavior  
What actually happened

## Environment
- OS: [e.g., Ubuntu 22.04]
- Rust version: [e.g., 1.73.0]
- FastResize version: [e.g., 0.1.0]
- Image details: [format, size, dimensions]

## Additional Context
Any other relevant information
```

**Feature Request Template:**
```markdown
## Feature Description
Clear description of the desired feature

## Use Case
Explain why this feature would be useful

## Proposed Implementation
Ideas for how this could be implemented

## Alternatives Considered
Other approaches you've considered
```

### Contributing Code

#### 1. Fork and Clone
```bash
git clone https://github.com/YOUR-USERNAME/resizer.git
cd resizer
git remote add upstream https://github.com/olereon/resizer.git
```

#### 2. Create a Feature Branch
```bash
git checkout -b feature/descriptive-name
# or
git checkout -b fix/issue-number-description
```

#### 3. Make Changes

**Code Style Guidelines:**
- Follow Rust standard formatting (`cargo fmt`)
- Use meaningful variable and function names
- Add documentation for public APIs
- Include unit tests for new functionality
- Update integration tests if needed

**Performance Considerations:**
- Profile performance-critical changes
- Avoid unnecessary allocations in hot paths
- Consider memory usage for large image processing
- Test with various image sizes and formats

#### 4. Test Your Changes
```bash
# Run all tests
cargo test

# Run specific test module
cargo test processing

# Run integration tests
cargo test --test integration

# Run performance benchmarks
cargo bench

# Test with sample images
./scripts/test-with-samples.sh
```

#### 5. Commit Your Changes
```bash
git add .
git commit -m "feat: add support for AVIF format"

# or for bug fixes
git commit -m "fix: handle large JPEG files without memory overflow"
```

**Commit Message Guidelines:**
- Use conventional commits format: `type: description`
- Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`
- Keep first line under 72 characters
- Include issue number if applicable: `fixes #123`

#### 6. Push and Create Pull Request
```bash
git push origin feature/descriptive-name
```

Create a pull request via GitHub with:
- Clear title and description
- Link to related issues
- Screenshots for UI changes
- Performance impact notes
- Breaking changes documentation

## Development Guidelines

### Project Structure

```
src/
├── main.rs           # CLI entry point
├── lib.rs            # Library interface
├── config/           # Configuration management
├── processing/       # Core image processing
├── parallel/         # Parallel processing logic
└── automation/       # Watch mode and batch jobs

tests/
├── integration/      # End-to-end tests
├── benchmarks/       # Performance tests
└── fixtures/         # Test images

docs/
├── api/             # API documentation
└── examples/        # Usage examples
```

### Code Quality Standards

**Rust Best Practices:**
```rust
// Use explicit error handling
fn process_image(path: &Path) -> Result<Image, ProcessingError> {
    // Implementation
}

// Document public APIs
/// Resizes an image using the specified parameters.
/// 
/// # Arguments
/// * `image` - The source image to resize
/// * `config` - Resize configuration parameters
/// 
/// # Returns
/// * `Ok(Image)` - Successfully resized image
/// * `Err(ProcessingError)` - If resizing fails
pub fn resize_image(image: &Image, config: &ResizeConfig) -> Result<Image, ProcessingError> {
    // Implementation
}

// Use appropriate error types
#[derive(Debug, thiserror::Error)]
pub enum ProcessingError {
    #[error("Failed to load image: {0}")]
    LoadError(String),
    #[error("Invalid resize parameters: {0}")]  
    InvalidParameters(String),
    #[error("Memory allocation failed")]
    OutOfMemory,
}
```

**Performance Guidelines:**
- Profile before optimizing
- Use `Cow<str>` for string handling
- Avoid unnecessary clones
- Prefer iterators over explicit loops
- Use `Arc` for shared data in parallel processing

**Testing Guidelines:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    
    #[test]
    fn test_resize_maintains_aspect_ratio() {
        let image = load_test_image("test.jpg");
        let config = ResizeConfig::new().width(800);
        let resized = resize_image(&image, &config).unwrap();
        
        let original_ratio = image.width() as f32 / image.height() as f32;
        let resized_ratio = resized.width() as f32 / resized.height() as f32;
        
        assert!((original_ratio - resized_ratio).abs() < 0.01);
    }
    
    #[test]
    fn test_memory_usage_large_batch() {
        // Test memory efficiency with large number of files
        let files = generate_test_files(1000);
        let initial_memory = get_memory_usage();
        
        process_batch(&files).unwrap();
        
        let final_memory = get_memory_usage();
        assert!(final_memory - initial_memory < 1_000_000_000); // <1GB increase
    }
}
```

### Adding New Features

#### Image Format Support
1. Add format detection in `src/processing/formats.rs`
2. Implement format-specific optimizations
3. Add comprehensive tests with sample images
4. Update documentation and CLI help

#### Performance Optimizations
1. Benchmark existing performance
2. Implement optimization
3. Verify performance improvement
4. Add regression tests
5. Document performance characteristics

#### CLI Features
1. Add argument parsing in `src/main.rs`
2. Update help text and documentation
3. Ensure backward compatibility
4. Add integration tests
5. Update shell completion scripts

### Testing

#### Test Categories

**Unit Tests** (`cargo test`)
- Algorithm correctness
- Error handling
- Edge cases
- Memory safety

**Integration Tests** (`cargo test --test integration`)
- End-to-end workflows
- CLI argument parsing
- File I/O operations
- Cross-platform compatibility

**Performance Tests** (`cargo bench`)
- Processing speed benchmarks
- Memory usage profiling
- Scalability testing
- Regression detection

#### Test Data

Use the provided test images in `tests/fixtures/`:
```bash
tests/fixtures/
├── small/       # <1MB images
├── medium/      # 1-10MB images  
├── large/       # 10-50MB images
├── formats/     # Various formats (JPEG, PNG, WebP, etc.)
└── edge-cases/  # Unusual dimensions, corrupted files
```

Create additional test images:
```bash
# Generate test images
./scripts/generate-test-images.sh

# Download sample images from internet
./scripts/download-samples.sh
```

## Documentation

### Code Documentation
- Document all public APIs with `///` comments
- Include examples in documentation
- Use `cargo doc --open` to preview documentation

### User Documentation
- Update README.md for new features
- Add examples to the examples directory
- Update CLI help text

### Architecture Documentation
- Document design decisions in ARCHITECTURE.md
- Update performance characteristics
- Explain trade-offs and alternatives

## Release Process

### Version Numbering
We follow [Semantic Versioning](https://semver.org/):
- **MAJOR**: Breaking changes
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

### Pre-release Checklist
- [ ] All tests pass
- [ ] Performance benchmarks show no regression
- [ ] Documentation is updated
- [ ] CHANGELOG.md is updated
- [ ] Cross-platform builds succeed
- [ ] Security audit passes (`cargo audit`)

### Release Steps
1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Create release commit: `git commit -m "chore: release v1.2.3"`
4. Create git tag: `git tag v1.2.3`
5. Push: `git push origin main --tags`
6. CI will build and publish release

## Community Guidelines

### Communication Channels
- **GitHub Issues**: Bug reports, feature requests
- **GitHub Discussions**: General questions, ideas
- **Pull Requests**: Code contributions
- **Wiki**: Community documentation

### Review Process
- All contributions require review from maintainers
- Address feedback constructively
- Squash commits before merging
- Update documentation as needed

### Recognition
Contributors are recognized in:
- CONTRIBUTORS.md file
- Release notes for significant contributions
- GitHub contributor graphs

## Getting Help

### Documentation Resources
- [Rust Book](https://doc.rust-lang.org/book/)
- [Image Processing in Rust](https://docs.rs/image/)
- [Parallel Processing with Rayon](https://docs.rs/rayon/)

### Project Resources
- README.md for user documentation
- CLAUDE.md for development context
- API documentation: `cargo doc --open`
- Examples directory for usage patterns

### Contact
- Create an issue for bugs or feature requests
- Start a discussion for questions or ideas
- Mention maintainers in PRs for review

---

Thank you for contributing to FastResize! Your contributions help make image processing faster and more accessible for everyone.