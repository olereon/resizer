//! Core image processing functionality

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tracing::debug;

use crate::config::{ResizeConfig, ResizeMode, ImageFormat, ProcessingProfile};
use crate::error::{Result, FastResizeError, ErrorContext};

pub mod resize;
pub mod formats;
pub mod memory;
pub mod validation;

pub use resize::*;
pub use formats::*;
pub use memory::*;
pub use validation::*;

/// Core processing engine for image operations
pub struct ProcessingEngine {
    memory_pool: Arc<MemoryPool>,
    validator: Arc<ImageValidator>,
}

impl ProcessingEngine {
    /// Create a new processing engine
    pub fn new() -> Self {
        Self {
            memory_pool: Arc::new(MemoryPool::new()),
            validator: Arc::new(ImageValidator::new()),
        }
    }

    /// Process a single file with the given configuration
    pub async fn process_file<P: AsRef<Path>>(
        &self,
        input_path: P,
        output_path: P,
        config: &ResizeConfig,
    ) -> Result<ProcessingResult> {
        let start_time = Instant::now();
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        debug!("Processing file: {:?} -> {:?}", input_path, output_path);

        // Validate input file
        self.validator.validate_file(input_path).await
            .with_file_context(input_path.to_path_buf())?;

        // Load image
        let (image, original_info) = self.load_image(input_path).await?;
        
        // Resize image
        let resized_image = self.resize_image(image, config, &original_info).await?;
        
        // Save image
        let output_info = self.save_image(&resized_image, output_path, config).await?;
        
        let processing_time = start_time.elapsed();
        
        Ok(ProcessingResult {
            input_path: input_path.to_path_buf(),
            output_path: output_path.to_path_buf(),
            original_info,
            output_info,
            processing_time,
            success: true,
            error: None,
        })
    }

    /// Process a file using a processing profile
    pub async fn process_file_with_profile<P: AsRef<Path>>(
        &self,
        input_path: P,
        output_dir: P,
        profile: &ProcessingProfile,
    ) -> Result<ProcessingResult> {
        let input_path = input_path.as_ref();
        let output_dir = output_dir.as_ref();

        // Generate output filename
        let input_filename = input_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| FastResizeError::validation(
                "Invalid input filename", 
                Some(input_path.to_path_buf())
            ))?;

        let output_filename = profile.naming.generate_filename(
            input_filename,
            profile.format,
        );
        
        let output_path = output_dir.join(output_filename);

        // Create resize config from profile
        let config = ResizeConfig {
            mode: profile.resize_mode.clone(),
            quality: profile.quality,
            format: profile.format,
        };

        self.process_file(input_path, &output_path, &config).await
    }

    /// Load an image from file
    async fn load_image(&self, path: &Path) -> Result<(image::DynamicImage, ImageInfo)> {
        debug!("Loading image: {:?}", path);

        // Read file metadata
        let metadata = fs::metadata(path).await
            .with_file_context(path.to_path_buf())?;
        let file_size = metadata.len();

        // Check file size limits
        const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024; // 500MB
        if file_size > MAX_FILE_SIZE {
            return Err(FastResizeError::file_too_large(
                file_size,
                MAX_FILE_SIZE,
                path.to_path_buf(),
            ));
        }

        // Determine if we should use memory mapping for large files
        let use_mmap = file_size > 100 * 1024 * 1024; // 100MB threshold

        let image = if use_mmap {
            self.load_image_mmap(path).await?
        } else {
            self.load_image_standard(path).await?
        };

        let info = ImageInfo {
            path: path.to_path_buf(),
            width: image.width(),
            height: image.height(),
            format: detect_format_from_path(path)?,
            file_size,
            pixel_count: (image.width() as u64) * (image.height() as u64),
        };

        debug!("Loaded image: {}x{} ({} pixels, {:.2}MB)", 
               info.width, info.height, info.pixel_count,
               info.file_size as f64 / 1024.0 / 1024.0);

        Ok((image, info))
    }

    /// Load image using standard file I/O
    async fn load_image_standard(&self, path: &Path) -> Result<image::DynamicImage> {
        let data = fs::read(path).await
            .with_file_context(path.to_path_buf())?;

        let image = tokio::task::spawn_blocking({
            let data = data.clone();
            let path = path.to_path_buf();
            move || -> Result<image::DynamicImage> {
                image::load_from_memory(&data)
                    .map_err(|e| FastResizeError::validation(
                        format!("Failed to decode image: {}", e),
                        Some(path),
                    ))
            }
        }).await
        .map_err(|e| FastResizeError::system(format!("Task join error: {}", e)))??;

        Ok(image)
    }

    /// Load image using memory mapping (for large files)
    async fn load_image_mmap(&self, path: &Path) -> Result<image::DynamicImage> {
        use memmap2::MmapOptions;
        use std::fs::File;

        debug!("Using memory mapping for large file: {:?}", path);

        let file = File::open(path)
            .with_file_context(path.to_path_buf())?;
        
        let mmap = unsafe {
            MmapOptions::new().map(&file)
                .with_file_context(path.to_path_buf())?
        };

        let image = tokio::task::spawn_blocking({
            let path = path.to_path_buf();
            move || -> Result<image::DynamicImage> {
                image::load_from_memory(&mmap)
                    .map_err(|e| FastResizeError::validation(
                        format!("Failed to decode memory-mapped image: {}", e),
                        Some(path),
                    ))
            }
        }).await
        .map_err(|e| FastResizeError::system(format!("Task join error: {}", e)))??;

        Ok(image)
    }

    /// Resize an image according to configuration
    async fn resize_image(
        &self,
        image: image::DynamicImage,
        config: &ResizeConfig,
        _original_info: &ImageInfo,
    ) -> Result<image::DynamicImage> {
        debug!("Resizing image: {} -> {:?}", 
               format!("{}x{}", image.width(), image.height()),
               config.mode);

        // Calculate target dimensions
        let (target_width, target_height) = calculate_dimensions(
            image.width(),
            image.height(),
            &config.mode,
        )?;

        debug!("Target dimensions: {}x{}", target_width, target_height);

        // Check if resize is actually needed
        if target_width == image.width() && target_height == image.height() {
            debug!("No resize needed, dimensions already match target");
            return Ok(image);
        }

        // Perform the resize operation
        let resized = tokio::task::spawn_blocking({
            let image = image.clone();
            move || -> Result<image::DynamicImage> {
                // Use high-quality filtering for better results
                let filter = image::imageops::FilterType::Lanczos3;
                
                let resized = image.resize(target_width, target_height, filter);
                
                Ok(resized)
            }
        }).await
        .map_err(|e| FastResizeError::system(format!("Task join error: {}", e)))??;

        debug!("Resize completed: {}x{} -> {}x{}", 
               image.width(), image.height(),
               resized.width(), resized.height());

        Ok(resized)
    }

    /// Save an image to file
    async fn save_image(
        &self,
        image: &image::DynamicImage,
        output_path: &Path,
        config: &ResizeConfig,
    ) -> Result<ImageInfo> {
        debug!("Saving image: {:?}", output_path);

        // Ensure output directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).await
                .with_file_context(output_path.to_path_buf())?;
        }

        // Determine output format
        let output_format = config.format
            .or_else(|| detect_format_from_path(output_path).ok())
            .unwrap_or(ImageFormat::Jpeg);

        debug!("Output format: {:?}, quality: {}", output_format, config.quality);

        // Save the image
        let file_size = tokio::task::spawn_blocking({
            let image = image.clone();
            let output_path = output_path.to_path_buf();
            let quality = config.quality;
            move || -> Result<u64> {
                // Convert quality to appropriate format
                match output_format {
                    ImageFormat::Jpeg => {
                        let mut output = std::fs::File::create(&output_path)
                            .with_file_context(output_path.clone())?;
                        
                        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                            &mut output, quality
                        );
                        
                        image.write_with_encoder(encoder)
                            .with_file_context(output_path.clone())?;
                    }
                    ImageFormat::Png => {
                        image.save(&output_path)
                            .with_file_context(output_path.clone())?;
                    }
                    ImageFormat::WebP => {
                        // Note: WebP encoding with quality control requires additional setup
                        image.save(&output_path)
                            .with_file_context(output_path.clone())?;
                    }
                    _ => {
                        image.save(&output_path)
                            .with_file_context(output_path.clone())?;
                    }
                }

                // Get file size
                let metadata = std::fs::metadata(&output_path)
                    .with_file_context(output_path.clone())?;
                
                Ok(metadata.len())
            }
        }).await
        .map_err(|e| FastResizeError::system(format!("Task join error: {}", e)))??;

        let info = ImageInfo {
            path: output_path.to_path_buf(),
            width: image.width(),
            height: image.height(),
            format: output_format,
            file_size,
            pixel_count: (image.width() as u64) * (image.height() as u64),
        };

        debug!("Saved image: {}x{} ({:.2}MB)", 
               info.width, info.height,
               info.file_size as f64 / 1024.0 / 1024.0);

        Ok(info)
    }
}

impl Default for ProcessingEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about an image file
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub file_size: u64,
    pub pixel_count: u64,
}

/// Result of processing an image
#[derive(Debug, Clone)]
pub struct ProcessingResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub original_info: ImageInfo,
    pub output_info: ImageInfo,
    pub processing_time: Duration,
    pub success: bool,
    pub error: Option<String>,
}

impl ProcessingResult {
    /// Create a failed processing result
    pub fn failed(
        input_path: PathBuf,
        error: FastResizeError,
        processing_time: Duration,
    ) -> Self {
        Self {
            input_path: input_path.clone(),
            output_path: PathBuf::new(),
            original_info: ImageInfo {
                path: input_path,
                width: 0,
                height: 0,
                format: ImageFormat::Jpeg,
                file_size: 0,
                pixel_count: 0,
            },
            output_info: ImageInfo {
                path: PathBuf::new(),
                width: 0,
                height: 0,
                format: ImageFormat::Jpeg,
                file_size: 0,
                pixel_count: 0,
            },
            processing_time,
            success: false,
            error: Some(error.user_message()),
        }
    }

    /// Get compression ratio (original size / output size)
    pub fn compression_ratio(&self) -> f64 {
        if self.output_info.file_size == 0 {
            return 1.0;
        }
        self.original_info.file_size as f64 / self.output_info.file_size as f64
    }

    /// Get size reduction percentage
    pub fn size_reduction(&self) -> f64 {
        if self.original_info.file_size == 0 {
            return 0.0;
        }
        let reduction = self.original_info.file_size.saturating_sub(self.output_info.file_size);
        (reduction as f64 / self.original_info.file_size as f64) * 100.0
    }

    /// Get processing speed in pixels per second
    pub fn pixels_per_second(&self) -> f64 {
        if self.processing_time.is_zero() {
            return 0.0;
        }
        self.original_info.pixel_count as f64 / self.processing_time.as_secs_f64()
    }
}

/// Calculate target dimensions based on resize mode
pub fn calculate_dimensions(
    original_width: u32,
    original_height: u32,
    mode: &ResizeMode,
) -> Result<(u32, u32)> {
    match mode {
        ResizeMode::Scale { factor } => {
            if *factor <= 0.0 {
                return Err(FastResizeError::invalid_parameters(
                    "Scale factor must be positive"
                ));
            }
            let width = (original_width as f32 * factor).round() as u32;
            let height = (original_height as f32 * factor).round() as u32;
            Ok((width.max(1), height.max(1)))
        }
        
        ResizeMode::Width { width } => {
            let aspect_ratio = original_height as f32 / original_width as f32;
            let height = (*width as f32 * aspect_ratio).round() as u32;
            Ok((*width, height.max(1)))
        }
        
        ResizeMode::Height { height } => {
            let aspect_ratio = original_width as f32 / original_height as f32;
            let width = (*height as f32 * aspect_ratio).round() as u32;
            Ok((width.max(1), *height))
        }
        
        ResizeMode::Fit { width, height } => {
            let original_aspect = original_width as f32 / original_height as f32;
            let target_aspect = *width as f32 / *height as f32;
            
            if original_aspect > target_aspect {
                // Fit to width
                let new_height = (*width as f32 / original_aspect).round() as u32;
                Ok((*width, new_height.max(1)))
            } else {
                // Fit to height
                let new_width = (*height as f32 * original_aspect).round() as u32;
                Ok((new_width.max(1), *height))
            }
        }
        
        ResizeMode::Fill { width, height } => {
            // For fill mode, we resize to fill the target dimensions
            // This may require cropping, which isn't implemented yet
            // For now, we just resize to fill (may change aspect ratio)
            Ok((*width, *height))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_dimensions_scale() {
        let result = calculate_dimensions(1000, 800, &ResizeMode::Scale { factor: 0.5 });
        assert_eq!(result.unwrap(), (500, 400));
    }

    #[test]
    fn test_calculate_dimensions_width() {
        let result = calculate_dimensions(1000, 800, &ResizeMode::Width { width: 500 });
        assert_eq!(result.unwrap(), (500, 400));
    }

    #[test]
    fn test_calculate_dimensions_height() {
        let result = calculate_dimensions(1000, 800, &ResizeMode::Height { height: 400 });
        assert_eq!(result.unwrap(), (500, 400));
    }

    #[test]
    fn test_calculate_dimensions_fit() {
        // Landscape image fitting in square
        let result = calculate_dimensions(1000, 800, &ResizeMode::Fit { width: 600, height: 600 });
        assert_eq!(result.unwrap(), (600, 480));
        
        // Portrait image fitting in square
        let result = calculate_dimensions(800, 1000, &ResizeMode::Fit { width: 600, height: 600 });
        assert_eq!(result.unwrap(), (480, 600));
    }

    #[test]
    fn test_processing_result_metrics() {
        let result = ProcessingResult {
            input_path: PathBuf::from("input.jpg"),
            output_path: PathBuf::from("output.jpg"),
            original_info: ImageInfo {
                path: PathBuf::from("input.jpg"),
                width: 1000,
                height: 800,
                format: ImageFormat::Jpeg,
                file_size: 1000000, // 1MB
                pixel_count: 800000,
            },
            output_info: ImageInfo {
                path: PathBuf::from("output.jpg"),
                width: 500,
                height: 400,
                format: ImageFormat::Jpeg,
                file_size: 250000, // 250KB
                pixel_count: 200000,
            },
            processing_time: Duration::from_secs(1),
            success: true,
            error: None,
        };

        assert!((result.compression_ratio() - 4.0).abs() < 0.1);
        assert!((result.size_reduction() - 75.0).abs() < 0.1);
        assert!((result.pixels_per_second() - 800000.0).abs() < 1.0);
    }
}