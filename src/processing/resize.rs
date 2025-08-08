//! Image resizing algorithms and utilities

use image::DynamicImage;
use crate::config::ResizeMode;
use crate::error::{Result, FastResizeError};
use tracing::debug;

/// High-quality image resizer with various algorithms
pub struct ImageResizer {
    filter: FilterType,
    preserve_aspect_ratio: bool,
}

/// Available resize filters
#[derive(Debug, Clone, Copy)]
pub enum FilterType {
    /// Nearest neighbor (fastest, lowest quality)
    Nearest,
    /// Triangle (linear interpolation)
    Triangle,
    /// Catmull-Rom cubic spline
    CatmullRom,
    /// Gaussian blur
    Gaussian,
    /// Lanczos with radius 3 (high quality, recommended)
    Lanczos3,
}

impl Default for FilterType {
    fn default() -> Self {
        Self::Lanczos3
    }
}

impl From<FilterType> for image::imageops::FilterType {
    fn from(filter: FilterType) -> Self {
        match filter {
            FilterType::Nearest => image::imageops::FilterType::Nearest,
            FilterType::Triangle => image::imageops::FilterType::Triangle,
            FilterType::CatmullRom => image::imageops::FilterType::CatmullRom,
            FilterType::Gaussian => image::imageops::FilterType::Gaussian,
            FilterType::Lanczos3 => image::imageops::FilterType::Lanczos3,
        }
    }
}

impl ImageResizer {
    /// Create a new resizer with default settings
    pub fn new() -> Self {
        Self {
            filter: FilterType::Lanczos3,
            preserve_aspect_ratio: true,
        }
    }

    /// Create a resizer with custom filter
    pub fn with_filter(filter: FilterType) -> Self {
        Self {
            filter,
            preserve_aspect_ratio: true,
        }
    }

    /// Set whether to preserve aspect ratio
    pub fn preserve_aspect_ratio(mut self, preserve: bool) -> Self {
        self.preserve_aspect_ratio = preserve;
        self
    }

    /// Resize an image according to the specified mode
    pub fn resize(&self, image: &DynamicImage, mode: &ResizeMode) -> Result<DynamicImage> {
        let (target_width, target_height) = self.calculate_target_dimensions(image, mode)?;

        debug!(
            "Resizing {}x{} -> {}x{} using {:?}",
            image.width(),
            image.height(),
            target_width,
            target_height,
            self.filter
        );

        // Check if resize is needed
        if target_width == image.width() && target_height == image.height() {
            return Ok(image.clone());
        }

        let resized = match mode {
            ResizeMode::Fill { .. } => {
                // For fill mode, we need to crop to maintain aspect ratio
                self.resize_and_crop(image, target_width, target_height)?
            }
            _ => {
                // Standard resize maintaining aspect ratio
                let filter: image::imageops::FilterType = self.filter.into();
                image.resize(target_width, target_height, filter)
            }
        };

        Ok(resized)
    }

    /// Calculate target dimensions based on resize mode
    fn calculate_target_dimensions(
        &self,
        image: &DynamicImage,
        mode: &ResizeMode,
    ) -> Result<(u32, u32)> {
        let original_width = image.width();
        let original_height = image.height();

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
                if *width == 0 {
                    return Err(FastResizeError::invalid_parameters(
                        "Width must be greater than 0"
                    ));
                }
                if self.preserve_aspect_ratio {
                    let aspect_ratio = original_height as f32 / original_width as f32;
                    let height = (*width as f32 * aspect_ratio).round() as u32;
                    Ok((*width, height.max(1)))
                } else {
                    Ok((*width, original_height))
                }
            }

            ResizeMode::Height { height } => {
                if *height == 0 {
                    return Err(FastResizeError::invalid_parameters(
                        "Height must be greater than 0"
                    ));
                }
                if self.preserve_aspect_ratio {
                    let aspect_ratio = original_width as f32 / original_height as f32;
                    let width = (*height as f32 * aspect_ratio).round() as u32;
                    Ok((width.max(1), *height))
                } else {
                    Ok((original_width, *height))
                }
            }

            ResizeMode::Fit { width, height } => {
                if *width == 0 || *height == 0 {
                    return Err(FastResizeError::invalid_parameters(
                        "Width and height must be greater than 0"
                    ));
                }

                let original_aspect = original_width as f32 / original_height as f32;
                let target_aspect = *width as f32 / *height as f32;

                if original_aspect > target_aspect {
                    // Image is wider, fit to width
                    let new_height = (*width as f32 / original_aspect).round() as u32;
                    Ok((*width, new_height.max(1)))
                } else {
                    // Image is taller, fit to height
                    let new_width = (*height as f32 * original_aspect).round() as u32;
                    Ok((new_width.max(1), *height))
                }
            }

            ResizeMode::Fill { width, height } => {
                if *width == 0 || *height == 0 {
                    return Err(FastResizeError::invalid_parameters(
                        "Width and height must be greater than 0"
                    ));
                }
                Ok((*width, *height))
            }
        }
    }

    /// Resize and crop to fill target dimensions exactly
    fn resize_and_crop(
        &self,
        image: &DynamicImage,
        target_width: u32,
        target_height: u32,
    ) -> Result<DynamicImage> {
        let original_width = image.width();
        let original_height = image.height();

        let original_aspect = original_width as f32 / original_height as f32;
        let target_aspect = target_width as f32 / target_height as f32;

        let (intermediate_width, intermediate_height) = if original_aspect > target_aspect {
            // Image is wider, scale to height and crop width
            let scale_factor = target_height as f32 / original_height as f32;
            let new_width = (original_width as f32 * scale_factor).round() as u32;
            (new_width, target_height)
        } else {
            // Image is taller, scale to width and crop height
            let scale_factor = target_width as f32 / original_width as f32;
            let new_height = (original_height as f32 * scale_factor).round() as u32;
            (target_width, new_height)
        };

        // First resize to intermediate size
        let filter: image::imageops::FilterType = self.filter.into();
        let resized = image.resize_exact(intermediate_width, intermediate_height, filter);

        // Then crop to exact target size
        let crop_x = if intermediate_width > target_width {
            (intermediate_width - target_width) / 2
        } else {
            0
        };

        let crop_y = if intermediate_height > target_height {
            (intermediate_height - target_height) / 2
        } else {
            0
        };

        let cropped = resized.crop_imm(crop_x, crop_y, target_width, target_height);
        Ok(cropped)
    }

    /// Resize with smart cropping (crop from center or focus point)
    pub fn resize_with_smart_crop(
        &self,
        image: &DynamicImage,
        target_width: u32,
        target_height: u32,
        focus_point: Option<(f32, f32)>, // (x, y) as percentages (0.0-1.0)
    ) -> Result<DynamicImage> {
        let original_width = image.width();
        let original_height = image.height();

        let original_aspect = original_width as f32 / original_height as f32;
        let target_aspect = target_width as f32 / target_height as f32;

        // Calculate intermediate size (larger of the two dimensions to avoid upscaling)
        let (intermediate_width, intermediate_height) = if original_aspect > target_aspect {
            let scale_factor = target_height as f32 / original_height as f32;
            let new_width = (original_width as f32 * scale_factor).round() as u32;
            (new_width, target_height)
        } else {
            let scale_factor = target_width as f32 / original_width as f32;
            let new_height = (original_height as f32 * scale_factor).round() as u32;
            (target_width, new_height)
        };

        // Resize to intermediate size
        let filter: image::imageops::FilterType = self.filter.into();
        let resized = image.resize_exact(intermediate_width, intermediate_height, filter);

        // Calculate crop position based on focus point
        let (focus_x, focus_y) = focus_point.unwrap_or((0.5, 0.5)); // Default to center

        let crop_x = if intermediate_width > target_width {
            let max_crop_x = intermediate_width - target_width;
            let ideal_crop_x = (focus_x * intermediate_width as f32 - target_width as f32 / 2.0).round() as u32;
            ideal_crop_x.min(max_crop_x).max(0)
        } else {
            0
        };

        let crop_y = if intermediate_height > target_height {
            let max_crop_y = intermediate_height - target_height;
            let ideal_crop_y = (focus_y * intermediate_height as f32 - target_height as f32 / 2.0).round() as u32;
            ideal_crop_y.min(max_crop_y).max(0)
        } else {
            0
        };

        let cropped = resized.crop_imm(crop_x, crop_y, target_width, target_height);
        Ok(cropped)
    }

    /// Apply unsharp mask filter to enhance details after resizing
    pub fn apply_unsharp_mask(
        &self,
        image: &DynamicImage,
        amount: f32,
        radius: f32,
        threshold: u8,
    ) -> Result<DynamicImage> {
        // This is a simplified unsharp mask implementation
        // In a production environment, you might want to use imageproc or similar
        if amount <= 0.0 || radius <= 0.0 {
            return Ok(image.clone());
        }

        debug!("Applying unsharp mask: amount={}, radius={}, threshold={}", 
               amount, radius, threshold);

        // For now, return original image
        // TODO: Implement proper unsharp mask using imageproc or custom implementation
        Ok(image.clone())
    }

    /// Optimize image for web delivery
    pub fn optimize_for_web(&self, image: &DynamicImage) -> Result<DynamicImage> {
        // Apply subtle sharpening for web display
        self.apply_unsharp_mask(image, 1.2, 1.0, 0)
    }
}

impl Default for ImageResizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for common resize operations
pub mod utils {
    use super::*;

    /// Create thumbnail with exact dimensions (may crop)
    pub fn create_thumbnail(
        image: &DynamicImage,
        size: u32,
        filter: Option<FilterType>,
    ) -> Result<DynamicImage> {
        let resizer = ImageResizer::with_filter(filter.unwrap_or(FilterType::Lanczos3));
        let mode = ResizeMode::Fill { 
            width: size, 
            height: size 
        };
        resizer.resize(image, &mode)
    }

    /// Resize for web with maximum dimensions
    pub fn resize_for_web(
        image: &DynamicImage,
        max_width: u32,
        max_height: u32,
    ) -> Result<DynamicImage> {
        let resizer = ImageResizer::new();
        let mode = ResizeMode::Fit { 
            width: max_width, 
            height: max_height 
        };
        let resized = resizer.resize(image, &mode)?;
        resizer.optimize_for_web(&resized)
    }

    /// Calculate memory usage for an image
    pub fn calculate_memory_usage(width: u32, height: u32, bytes_per_pixel: u32) -> u64 {
        (width as u64) * (height as u64) * (bytes_per_pixel as u64)
    }

    /// Check if resize will require significant memory
    pub fn is_memory_intensive(
        original_width: u32,
        original_height: u32,
        target_width: u32,
        target_height: u32,
    ) -> bool {
        const HIGH_MEMORY_THRESHOLD: u64 = 100_000_000; // 100MB (25M pixels * 4 bytes)
        
        let original_pixels = (original_width as u64) * (original_height as u64);
        let target_pixels = (target_width as u64) * (target_height as u64);
        let max_pixels = original_pixels.max(target_pixels);
        
        max_pixels * 4 > HIGH_MEMORY_THRESHOLD // Assume 4 bytes per pixel (RGBA)
    }

    /// Suggest optimal filter based on operation type
    pub fn suggest_filter(
        original_width: u32,
        original_height: u32,
        target_width: u32,
        target_height: u32,
    ) -> FilterType {
        let scale_factor = {
            let x_scale = target_width as f32 / original_width as f32;
            let y_scale = target_height as f32 / original_height as f32;
            x_scale.min(y_scale)
        };

        match scale_factor {
            f if f >= 2.0 => FilterType::Lanczos3,      // Upscaling - high quality
            f if f >= 0.5 => FilterType::Lanczos3,      // Moderate scaling
            f if f >= 0.25 => FilterType::CatmullRom,   // Downscaling
            _ => FilterType::Triangle,                   // Heavy downscaling - faster
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn create_test_image(width: u32, height: u32) -> DynamicImage {
        let img = ImageBuffer::from_fn(width, height, |x, y| {
            let intensity = ((x + y) % 255) as u8;
            Rgb([intensity, intensity, intensity])
        });
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn test_resizer_creation() {
        let resizer = ImageResizer::new();
        assert!(matches!(resizer.filter, FilterType::Lanczos3));
        assert!(resizer.preserve_aspect_ratio);

        let custom_resizer = ImageResizer::with_filter(FilterType::Nearest)
            .preserve_aspect_ratio(false);
        assert!(matches!(custom_resizer.filter, FilterType::Nearest));
        assert!(!custom_resizer.preserve_aspect_ratio);
    }

    #[test]
    fn test_dimension_calculation() {
        let resizer = ImageResizer::new();
        let image = create_test_image(1000, 800);

        // Scale mode
        let (w, h) = resizer.calculate_target_dimensions(
            &image, 
            &ResizeMode::Scale { factor: 0.5 }
        ).unwrap();
        assert_eq!((w, h), (500, 400));

        // Width mode
        let (w, h) = resizer.calculate_target_dimensions(
            &image,
            &ResizeMode::Width { width: 500 }
        ).unwrap();
        assert_eq!((w, h), (500, 400));

        // Height mode
        let (w, h) = resizer.calculate_target_dimensions(
            &image,
            &ResizeMode::Height { height: 400 }
        ).unwrap();
        assert_eq!((w, h), (500, 400));

        // Fit mode
        let (w, h) = resizer.calculate_target_dimensions(
            &image,
            &ResizeMode::Fit { width: 600, height: 600 }
        ).unwrap();
        assert_eq!((w, h), (600, 480));

        // Fill mode
        let (w, h) = resizer.calculate_target_dimensions(
            &image,
            &ResizeMode::Fill { width: 600, height: 600 }
        ).unwrap();
        assert_eq!((w, h), (600, 600));
    }

    #[test]
    fn test_basic_resize() {
        let resizer = ImageResizer::new();
        let image = create_test_image(1000, 800);
        
        let resized = resizer.resize(&image, &ResizeMode::Scale { factor: 0.5 }).unwrap();
        assert_eq!(resized.width(), 500);
        assert_eq!(resized.height(), 400);
    }

    #[test]
    fn test_no_resize_needed() {
        let resizer = ImageResizer::new();
        let image = create_test_image(100, 100);
        
        // Same dimensions - should return clone
        let resized = resizer.resize(&image, &ResizeMode::Width { width: 100 }).unwrap();
        assert_eq!(resized.width(), 100);
        assert_eq!(resized.height(), 100);
    }

    #[test]
    fn test_invalid_parameters() {
        let resizer = ImageResizer::new();
        let image = create_test_image(100, 100);

        // Zero scale factor
        assert!(resizer.resize(&image, &ResizeMode::Scale { factor: 0.0 }).is_err());

        // Zero dimensions
        assert!(resizer.resize(&image, &ResizeMode::Width { width: 0 }).is_err());
        assert!(resizer.resize(&image, &ResizeMode::Height { height: 0 }).is_err());
        assert!(resizer.resize(&image, &ResizeMode::Fit { width: 0, height: 100 }).is_err());
    }

    #[test]
    fn test_smart_crop() {
        let resizer = ImageResizer::new();
        let image = create_test_image(1000, 800);

        // Center crop
        let cropped = resizer.resize_with_smart_crop(&image, 500, 500, None).unwrap();
        assert_eq!(cropped.width(), 500);
        assert_eq!(cropped.height(), 500);

        // Off-center crop
        let cropped = resizer.resize_with_smart_crop(&image, 500, 500, Some((0.2, 0.2))).unwrap();
        assert_eq!(cropped.width(), 500);
        assert_eq!(cropped.height(), 500);
    }

    #[test]
    fn test_utility_functions() {
        let image = create_test_image(1000, 800);

        // Thumbnail creation
        let thumbnail = utils::create_thumbnail(&image, 150, None).unwrap();
        assert_eq!(thumbnail.width(), 150);
        assert_eq!(thumbnail.height(), 150);

        // Web resize
        let web_image = utils::resize_for_web(&image, 1920, 1080).unwrap();
        assert!(web_image.width() <= 1920);
        assert!(web_image.height() <= 1080);

        // Memory usage calculation
        let memory = utils::calculate_memory_usage(1920, 1080, 4);
        assert_eq!(memory, 1920 * 1080 * 4);

        // Memory intensive check
        assert!(utils::is_memory_intensive(5000, 5000, 5000, 5000));
        assert!(!utils::is_memory_intensive(100, 100, 100, 100));

        // Filter suggestions
        let upscale_filter = utils::suggest_filter(100, 100, 500, 500);
        assert!(matches!(upscale_filter, FilterType::Lanczos3));

        let downscale_filter = utils::suggest_filter(1000, 1000, 100, 100);
        assert!(matches!(downscale_filter, FilterType::Triangle));
    }

    #[test]
    fn test_filter_conversion() {
        let filters = [
            FilterType::Nearest,
            FilterType::Triangle,
            FilterType::CatmullRom,
            FilterType::Gaussian,
            FilterType::Lanczos3,
        ];

        for filter in &filters {
            let _: image::imageops::FilterType = (*filter).into();
        }
    }
}