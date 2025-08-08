//! FastResize CLI - High-Performance Batch Image Resizer
//!
//! A lightning-fast, memory-efficient command-line tool for batch image resizing,
//! designed for automation workflows and processing large volumes of images.

use std::path::PathBuf;
use std::process;
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use tracing::{info, warn, error, debug};

use fastresize::{
    Config, ProcessingEngine, ResizeConfig, ResizeMode, ImageFormat,
    init,
};

/// FastResize - High-Performance Batch Image Resizer
#[derive(Parser)]
#[command(
    name = "fastresize",
    version,
    about = "Lightning-fast batch image resizer for automation workflows",
    long_about = "FastResize is a high-performance, memory-efficient command-line tool for batch \
                  image resizing. Built in Rust for maximum speed and reliability, it's designed \
                  for automation workflows, CI/CD pipelines, and processing large volumes of images.",
    arg_required_else_help = false
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Input file or directory
    #[arg(short, long, value_name = "PATH")]
    input: Option<PathBuf>,

    /// Output directory
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Scale factor (0.1-10.0)
    #[arg(short, long, value_name = "FACTOR", conflicts_with_all = ["width", "height"])]
    scale: Option<f32>,

    /// Target width in pixels
    #[arg(short, long, value_name = "PIXELS", conflicts_with_all = ["scale", "height"])]
    width: Option<u32>,

    /// Target height in pixels  
    #[arg(short = 'H', long, value_name = "PIXELS", conflicts_with_all = ["scale", "width"])]
    height: Option<u32>,

    /// Fit within dimensions (width x height)
    #[arg(long, value_name = "WxH", value_parser = parse_dimensions, conflicts_with_all = ["scale", "width", "height", "fill"])]
    fit: Option<(u32, u32)>,

    /// Fill dimensions exactly (may crop)
    #[arg(long, value_name = "WxH", value_parser = parse_dimensions, conflicts_with_all = ["scale", "width", "height", "fit"])]
    fill: Option<(u32, u32)>,

    /// Output quality (1-100)
    #[arg(short, long, default_value = "90", value_name = "QUALITY")]
    quality: u8,

    /// Output format
    #[arg(short, long, value_enum, value_name = "FORMAT")]
    format: Option<CliImageFormat>,

    /// Number of threads (default: auto-detect)
    #[arg(short, long, value_name = "COUNT")]
    threads: Option<usize>,

    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Processing profile name
    #[arg(short, long, value_name = "NAME")]
    profile: Option<String>,

    /// Process directories recursively
    #[arg(short = 'R', long)]
    recursive: bool,

    /// Watch input directory for changes
    #[arg(long)]
    watch: bool,

    /// Show what would be processed without actually processing
    #[arg(long)]
    dry_run: bool,

    /// Output progress as JSON
    #[arg(long)]
    json: bool,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Quiet mode (errors only)
    #[arg(short = 'Q', long, conflicts_with = "verbose")]
    quiet: bool,
}

/// Available subcommands
#[derive(Subcommand)]
enum Commands {
    /// List available processing profiles
    Profiles {
        /// Show detailed profile information
        #[arg(long)]
        detailed: bool,
    },
    /// Validate configuration file
    Config {
        /// Configuration file to validate
        file: PathBuf,
    },
    /// Generate example configuration file
    ExampleConfig {
        /// Output file path
        #[arg(short, long, default_value = "fastresize.toml")]
        output: PathBuf,
        /// Use YAML format instead of TOML
        #[arg(long)]
        yaml: bool,
    },
    /// Show system information and capabilities
    Info,
    /// Run performance benchmarks
    Benchmark {
        /// Test image size (small, medium, large)
        #[arg(short, long, default_value = "medium")]
        size: String,
        /// Number of iterations
        #[arg(short, long, default_value = "10")]
        iterations: u32,
    },
}

/// CLI-compatible image format enum
#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliImageFormat {
    Jpeg,
    Png,
    Webp,
    Gif,
    Tiff,
    Bmp,
}

impl From<CliImageFormat> for ImageFormat {
    fn from(format: CliImageFormat) -> Self {
        match format {
            CliImageFormat::Jpeg => ImageFormat::Jpeg,
            CliImageFormat::Png => ImageFormat::Png,
            CliImageFormat::Webp => ImageFormat::WebP,
            CliImageFormat::Gif => ImageFormat::Gif,
            CliImageFormat::Tiff => ImageFormat::Tiff,
            CliImageFormat::Bmp => ImageFormat::Bmp,
        }
    }
}

/// Parse dimension string (e.g., "1920x1080")
fn parse_dimensions(s: &str) -> Result<(u32, u32), String> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() != 2 {
        return Err("Dimensions must be in format 'WIDTHxHEIGHT' (e.g., '1920x1080')".to_string());
    }

    let width = parts[0].parse::<u32>()
        .map_err(|_| "Invalid width value".to_string())?;
    let height = parts[1].parse::<u32>()
        .map_err(|_| "Invalid height value".to_string())?;

    if width == 0 || height == 0 {
        return Err("Width and height must be greater than 0".to_string());
    }

    Ok((width, height))
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    
    // Initialize logging based on verbosity
    let log_level = if cli.quiet {
        "error"
    } else if cli.verbose {
        "debug"
    } else {
        "info"
    };

    std::env::set_var("RUST_LOG", log_level);

    // Handle subcommands
    if let Some(command) = cli.command {
        if let Err(e) = handle_subcommand(command).await {
            eprintln!("{}: {}", style("Error").red().bold(), e);
            process::exit(1);
        }
        return;
    }

    // Initialize FastResize
    if let Err(e) = init() {
        eprintln!("{}: Failed to initialize FastResize: {}", 
                 style("Error").red().bold(), e);
        process::exit(1);
    }

    // Validate required arguments for main operation
    let (input_path, output_path) = match (&cli.input, &cli.output) {
        (Some(input), Some(output)) => (input.clone(), output.clone()),
        _ => {
            eprintln!("{}: Input and output paths are required", 
                     style("Error").red().bold());
            eprintln!("Run with --help for usage information");
            process::exit(1);
        }
    };

    // Load configuration if provided
    let config = if let Some(ref config_path) = cli.config {
        match Config::from_file(&config_path) {
            Ok(config) => {
                info!("Loaded configuration from: {:?}", config_path);
                Some(config)
            }
            Err(e) => {
                error!("Failed to load configuration: {}", e);
                process::exit(1);
            }
        }
    } else {
        None
    };

    // Create resize configuration
    let resize_config = match create_resize_config(&cli) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}: {}", style("Error").red().bold(), e);
            process::exit(1);
        }
    };

    // Check for watch mode
    if cli.watch {
        if let Err(e) = run_watch_mode(&input_path, &output_path, &resize_config, &config).await {
            eprintln!("{}: Watch mode failed: {}", style("Error").red().bold(), e);
            process::exit(1);
        }
    } else {
        // Run single batch processing
        let start_time = Instant::now();
        match run_batch_processing(&cli, &input_path, &output_path, &resize_config, &config).await {
            Ok(results) => {
                let duration = start_time.elapsed();
                print_summary(&results, duration, cli.json);
            }
            Err(e) => {
                eprintln!("{}: Processing failed: {}", style("Error").red().bold(), e);
                process::exit(1);
            }
        }
    }
}

/// Handle subcommands
async fn handle_subcommand(command: Commands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Commands::Profiles { detailed } => {
            show_profiles(detailed);
        }
        Commands::Config { file } => {
            validate_config_file(&file)?;
        }
        Commands::ExampleConfig { output, yaml } => {
            generate_example_config(&output, yaml)?;
        }
        Commands::Info => {
            show_system_info().await;
        }
        Commands::Benchmark { size, iterations } => {
            run_benchmark(&size, iterations).await?;
        }
    }
    Ok(())
}

/// Create resize configuration from CLI arguments
fn create_resize_config(cli: &Cli) -> Result<ResizeConfig, String> {
    let mode = if let Some(factor) = cli.scale {
        if factor <= 0.0 || factor > 10.0 {
            return Err("Scale factor must be between 0.1 and 10.0".to_string());
        }
        ResizeMode::Scale { factor }
    } else if let Some(width) = cli.width {
        ResizeMode::Width { width }
    } else if let Some(height) = cli.height {
        ResizeMode::Height { height }
    } else if let Some((width, height)) = cli.fit {
        ResizeMode::Fit { width, height }
    } else if let Some((width, height)) = cli.fill {
        ResizeMode::Fill { width, height }
    } else {
        return Err("Must specify resize mode: --scale, --width, --height, --fit, or --fill".to_string());
    };

    if cli.quality == 0 || cli.quality > 100 {
        return Err("Quality must be between 1 and 100".to_string());
    }

    Ok(ResizeConfig {
        mode,
        quality: cli.quality,
        format: cli.format.map(Into::into),
    })
}

/// Run batch processing
async fn run_batch_processing(
    cli: &Cli,
    input_path: &std::path::Path,
    output_path: &std::path::Path,
    resize_config: &ResizeConfig,
    _config: &Option<Config>,
) -> Result<BatchResults, Box<dyn std::error::Error>> {
    
    info!("Starting batch processing");
    info!("Input: {:?}", input_path);
    info!("Output: {:?}", output_path);
    info!("Mode: {:?}", resize_config.mode);

    // Discover input files
    let files = discover_files(input_path, cli.recursive).await?;
    
    if files.is_empty() {
        return Err("No valid image files found in input path".into());
    }

    info!("Found {} files to process", files.len());

    if cli.dry_run {
        println!("{} files would be processed:", style(files.len()).bold());
        for file in &files {
            println!("  {}", file.display());
        }
        return Ok(BatchResults::default());
    }

    // Create processing engine
    let engine = ProcessingEngine::new();

    // Set up progress bar
    let progress = if !cli.json && !cli.quiet {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({per_sec}, {eta})")?
                .progress_chars("#>-")
        );
        Some(pb)
    } else {
        None
    };

    // Process files
    let mut results = BatchResults::default();
    
    for (_index, file_path) in files.iter().enumerate() {
        let output_file_path = generate_output_path(file_path, input_path, output_path, resize_config);
        
        if let Some(pb) = &progress {
            pb.set_message(format!("Processing: {}", file_path.file_name().unwrap_or_default().to_string_lossy()));
        }

        let _file_start = Instant::now();
        match engine.process_file(file_path, &output_file_path, resize_config).await {
            Ok(result) => {
                results.successful += 1;
                results.total_input_size += result.original_info.file_size;
                results.total_output_size += result.output_info.file_size;
                
                if cli.json {
                    // Note: ProcessingResult needs Serialize trait
                    debug!("JSON output requested for successful file");
                }
            }
            Err(e) => {
                results.failed += 1;
                let error_msg = format!("Failed to process {}: {}", file_path.display(), e);
                
                if cli.json {
                    // Note: ProcessingResult needs Serialize trait
                    debug!("JSON output requested for failed file");
                } else {
                    warn!("{}", error_msg);
                }
            }
        }

        if let Some(pb) = &progress {
            pb.inc(1);
        }
    }

    if let Some(pb) = &progress {
        pb.finish_with_message("Processing complete");
    }

    Ok(results)
}

/// Discover input files
async fn discover_files(
    input_path: &std::path::Path,
    recursive: bool,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    use fastresize::processing::formats::is_supported_input_format;
    use tokio::fs;
    
    let mut files = Vec::new();

    if input_path.is_file() {
        // Single file
        files.push(input_path.to_path_buf());
    } else if input_path.is_dir() {
        // Directory
        let mut entries = fs::read_dir(input_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if is_supported_input_format(ext) {
                        files.push(path);
                    }
                }
            } else if recursive && path.is_dir() {
                // Recursively process subdirectories
                let subdir_files = Box::pin(discover_files(&path, true)).await?;
                files.extend(subdir_files);
            }
        }
    } else {
        return Err(format!("Input path does not exist: {}", input_path.display()).into());
    }

    // Sort files for consistent processing order
    files.sort();
    Ok(files)
}

/// Generate output file path
fn generate_output_path(
    input_file: &std::path::Path,
    input_root: &std::path::Path,
    output_root: &std::path::Path,
    resize_config: &ResizeConfig,
) -> PathBuf {
    let relative_path = input_file.strip_prefix(input_root).unwrap_or(input_file);
    let mut output_path = output_root.join(relative_path);
    
    // Change extension if format conversion is specified
    if let Some(format) = resize_config.format {
        output_path.set_extension(format.extension());
    }
    
    output_path
}

/// Run watch mode
async fn run_watch_mode(
    _input_path: &std::path::Path,
    _output_path: &std::path::Path,
    _resize_config: &ResizeConfig,
    _config: &Option<Config>,
) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Implement watch mode
    println!("{}: Watch mode not yet implemented", style("Info").blue().bold());
    Ok(())
}

/// Show available profiles
fn show_profiles(detailed: bool) {
    println!("{}", style("Available Processing Profiles:").bold());
    println!();

    // For now, show basic profile information
    let profile_info = vec![
        ("thumbnail", "Square thumbnails (150x150, 85% quality)"),
        ("web", "Web optimized (1920px width, 80% quality)"),
        ("mobile", "Mobile friendly (800px width, 75% quality)"),
        ("print", "High quality for print (300 DPI, 95% quality)"),
    ];
    
    for (name, description) in &profile_info {
        println!("{}", style(name).cyan().bold());
        if detailed {
            println!("  {}", description);
            println!();
        }
    }

    if !detailed {
        println!();
        println!("Use {} for detailed information", style("--detailed").dim());
    }
}

/// Validate configuration file
fn validate_config_file(file_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_file(file_path)?;
    config.validate()?;
    
    println!("{}: Configuration file is valid", style("Success").green().bold());
    println!("Profiles: {}", config.profiles.len());
    println!("Watch folders: {}", config.automation.watch_folders.len());
    
    Ok(())
}

/// Generate example configuration file
fn generate_example_config(
    output_path: &std::path::Path,
    use_yaml: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::default();
    config.to_file(output_path)?;
    
    let format = if use_yaml { "YAML" } else { "TOML" };
    println!("{}: Generated example {} configuration: {}", 
             style("Success").green().bold(), 
             format,
             output_path.display());
    
    Ok(())
}

/// Show system information
async fn show_system_info() {
    use sysinfo::{System, SystemExt, CpuExt};
    
    println!("{}", style("FastResize System Information").bold());
    println!();
    
    // Version information
    println!("{}: {}", style("Version").bold(), env!("CARGO_PKG_VERSION"));
    println!("{}: {}", style("Build").bold(), "Release"); // TODO: Add build info
    println!();
    
    // System information
    let mut system = System::new_all();
    system.refresh_all();
    
    println!("{}", style("System:").bold());
    if let Some(name) = system.name() {
        println!("  OS: {}", name);
    }
    if let Some(version) = system.os_version() {
        println!("  Version: {}", version);
    }
    println!("  CPUs: {}", system.cpus().len());
    if let Some(cpu) = system.cpus().first() {
        println!("  CPU: {} ({:.2} GHz)", cpu.brand(), cpu.frequency() as f64 / 1000.0);
    }
    println!("  Memory: {:.2} GB total, {:.2} GB available", 
             system.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
             system.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0);
    println!();
    
    // Supported formats
    println!("{}", style("Supported Formats:").bold());
    println!("  Input: JPEG, PNG, WebP, GIF, TIFF, BMP");
    println!("  Output: JPEG, PNG, WebP, GIF, TIFF, BMP");
    println!();
    
    // Feature support
    println!("{}", style("Features:").bold());
    println!("  ✓ Parallel processing");
    println!("  ✓ Memory optimization");
    println!("  ✓ Large file support");
    println!("  ✓ Format conversion");
    println!("  - Watch mode (coming soon)");
    println!("  - GPU acceleration (experimental)");
}

/// Run performance benchmark
async fn run_benchmark(
    _size: &str,
    _iterations: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Implement benchmarking
    println!("{}: Benchmarking not yet implemented", style("Info").blue().bold());
    Ok(())
}

/// Print processing summary
fn print_summary(results: &BatchResults, duration: std::time::Duration, json_output: bool) {
    if json_output {
        // Note: BatchResults needs Serialize trait for full JSON support
        debug!("JSON output requested for summary");
        return;
    }

    println!();
    println!("{}", style("Processing Summary:").bold());
    println!("  {}: {}", style("Processed").green(), results.successful);
    if results.failed > 0 {
        println!("  {}: {}", style("Failed").red(), results.failed);
    }
    println!("  {}: {:.2}s", style("Duration").blue(), duration.as_secs_f64());
    
    if results.successful > 0 {
        let compression_ratio = results.total_input_size as f64 / results.total_output_size.max(1) as f64;
        let size_reduction = ((results.total_input_size.saturating_sub(results.total_output_size)) as f64 / results.total_input_size as f64) * 100.0;
        
        println!("  {}: {:.2}MB → {:.2}MB", 
                 style("Size").cyan(),
                 results.total_input_size as f64 / 1024.0 / 1024.0,
                 results.total_output_size as f64 / 1024.0 / 1024.0);
        println!("  {}: {:.1}x ({:.1}% reduction)", 
                 style("Compression").cyan(), 
                 compression_ratio, size_reduction);
        
        let files_per_second = results.successful as f64 / duration.as_secs_f64();
        println!("  {}: {:.1} files/sec", style("Speed").cyan(), files_per_second);
    }
}

/// Batch processing results
#[derive(Default, Debug, serde::Serialize)]
struct BatchResults {
    successful: u32,
    failed: u32,
    total_input_size: u64,
    total_output_size: u64,
}