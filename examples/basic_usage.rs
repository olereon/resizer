//! Basic usage example for the FastResize library

use fastresize::{init, ProcessingEngine, ResizeConfig, ResizeMode};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the library
    init()?;

    // Create a processing engine
    let engine = ProcessingEngine::new();

    // Configure resize operation
    let config = ResizeConfig::new()
        .width(1920)
        .quality(85);

    // Process a single image
    let result = engine.process_file(
        Path::new("input.jpg"),
        Path::new("output.jpg"),
        &config
    ).await?;

    println!("Successfully resized image:");
    println!("  Original: {}x{} ({:.2} MB)",
             result.original_info.width,
             result.original_info.height,
             result.original_info.file_size as f64 / 1024.0 / 1024.0);
    
    println!("  Resized: {}x{} ({:.2} MB)",
             result.output_info.width,
             result.output_info.height,
             result.output_info.file_size as f64 / 1024.0 / 1024.0);
    
    println!("  Compression: {:.1}x reduction",
             result.compression_ratio());
    
    println!("  Processing time: {:.2}s",
             result.processing_time.as_secs_f64());

    Ok(())
}