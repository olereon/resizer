//! Batch processing example with parallel execution

use fastresize::{init, parallel::ParallelProcessor, ResizeConfig, ResizeMode};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the library
    init()?;

    // Create a parallel processor with 4 concurrent workers
    let processor = ParallelProcessor::new(Some(4));

    // Configure resize operation
    let config = ResizeConfig::new()
        .fit(1920, 1080)
        .quality(85);

    // Collect input files (you would typically scan a directory)
    let files = vec![
        PathBuf::from("photo1.jpg"),
        PathBuf::from("photo2.jpg"),
        PathBuf::from("photo3.jpg"),
        // ... more files
    ];

    println!("Processing {} files in parallel...", files.len());

    // Process all files in parallel
    let result = processor.process_batch(
        files,
        &PathBuf::from("output/"),
        &config
    ).await?;

    // Print summary
    result.print_summary();

    Ok(())
}