//! Image format detection and handling

use std::path::Path;
use crate::config::ImageFormat;
use crate::error::{Result, FastResizeError};

/// Detect image format from file extension
pub fn detect_format_from_path<P: AsRef<Path>>(path: P) -> Result<ImageFormat> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| FastResizeError::unsupported_format(
            "Unknown".to_string(), 
            Some(path.to_path_buf())
        ))?;

    match extension.to_lowercase().as_str() {
        "jpg" | "jpeg" => Ok(ImageFormat::Jpeg),
        "png" => Ok(ImageFormat::Png),
        "webp" => Ok(ImageFormat::WebP),
        "gif" => Ok(ImageFormat::Gif),
        "tiff" | "tif" => Ok(ImageFormat::Tiff),
        "bmp" => Ok(ImageFormat::Bmp),
        _ => Err(FastResizeError::unsupported_format(
            extension.to_string(),
            Some(path.to_path_buf())
        )),
    }
}

/// Detect image format from file header (magic bytes)
pub fn detect_format_from_header(data: &[u8]) -> Result<ImageFormat> {
    if data.len() < 12 {
        return Err(FastResizeError::validation(
            "File too small to determine format".to_string(),
            None,
        ));
    }

    // JPEG: FF D8 FF
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Ok(ImageFormat::Jpeg);
    }

    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Ok(ImageFormat::Png);
    }

    // GIF: GIF87a or GIF89a
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Ok(ImageFormat::Gif);
    }

    // WebP: RIFF....WEBP
    if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return Ok(ImageFormat::WebP);
    }

    // TIFF: II*. (little-endian) or MM.* (big-endian)
    if data.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
        return Ok(ImageFormat::Tiff);
    }

    // BMP: BM
    if data.starts_with(b"BM") {
        return Ok(ImageFormat::Bmp);
    }

    Err(FastResizeError::unsupported_format(
        "Unknown (magic bytes)".to_string(),
        None,
    ))
}

/// Convert our ImageFormat to image crate format
impl From<ImageFormat> for image::ImageFormat {
    fn from(format: ImageFormat) -> Self {
        match format {
            ImageFormat::Jpeg => image::ImageFormat::Jpeg,
            ImageFormat::Png => image::ImageFormat::Png,
            ImageFormat::WebP => image::ImageFormat::WebP,
            ImageFormat::Gif => image::ImageFormat::Gif,
            ImageFormat::Tiff => image::ImageFormat::Tiff,
            ImageFormat::Bmp => image::ImageFormat::Bmp,
        }
    }
}

/// Get supported input formats
pub fn supported_input_formats() -> &'static [&'static str] {
    &["jpg", "jpeg", "png", "webp", "gif", "tiff", "tif", "bmp"]
}

/// Get supported output formats
pub fn supported_output_formats() -> &'static [&'static str] {
    &["jpg", "jpeg", "png", "webp", "gif", "tiff", "bmp"]
}

/// Check if a file extension is supported for input
pub fn is_supported_input_format(extension: &str) -> bool {
    supported_input_formats()
        .iter()
        .any(|&fmt| fmt.eq_ignore_ascii_case(extension))
}

/// Check if a file extension is supported for output
pub fn is_supported_output_format(extension: &str) -> bool {
    supported_output_formats()
        .iter()
        .any(|&fmt| fmt.eq_ignore_ascii_case(extension))
}

/// Get optimal quality settings for different formats
pub fn get_optimal_quality(format: ImageFormat, target_quality: u8) -> u8 {
    match format {
        ImageFormat::Jpeg => {
            // JPEG quality mapping (1-100)
            target_quality.clamp(1, 100)
        }
        ImageFormat::WebP => {
            // WebP quality mapping (1-100)
            target_quality.clamp(1, 100)
        }
        ImageFormat::Png => {
            // PNG is lossless, but we can use compression level
            // Map quality to compression (inverse relationship)
            100 - target_quality.clamp(1, 100)
        }
        ImageFormat::Gif => {
            // GIF doesn't have traditional quality settings
            100 // Maximum quality
        }
        ImageFormat::Tiff => {
            // TIFF can be lossless or lossy
            target_quality.clamp(1, 100)
        }
        ImageFormat::Bmp => {
            // BMP is typically uncompressed
            100 // Maximum quality
        }
    }
}

/// Get estimated file size multiplier for different formats
pub fn get_size_multiplier(format: ImageFormat, quality: u8) -> f32 {
    match format {
        ImageFormat::Jpeg => {
            // JPEG compression ratio based on quality
            match quality {
                1..=20 => 0.05,   // Very high compression
                21..=40 => 0.10,  // High compression
                41..=60 => 0.15,  // Medium compression
                61..=80 => 0.25,  // Low compression
                81..=90 => 0.40,  // Minimal compression
                91..=100 => 0.60, // Very low compression
                _ => 0.25,
            }
        }
        ImageFormat::WebP => {
            // WebP is generally more efficient than JPEG
            match quality {
                1..=20 => 0.03,
                21..=40 => 0.08,
                41..=60 => 0.12,
                61..=80 => 0.20,
                81..=90 => 0.35,
                91..=100 => 0.50,
                _ => 0.20,
            }
        }
        ImageFormat::Png => {
            // PNG is lossless but varies by content
            0.80 // Typically larger than JPEG
        }
        ImageFormat::Gif => {
            // GIF with 256 colors max
            0.30
        }
        ImageFormat::Tiff => {
            // TIFF can be lossless
            1.00
        }
        ImageFormat::Bmp => {
            // BMP is uncompressed
            3.00 // Much larger
        }
    }
}

/// Format-specific optimization settings
pub struct FormatOptimization {
    pub progressive: bool,
    pub optimize_for_web: bool,
    pub preserve_metadata: bool,
    pub color_space: ColorSpace,
}

#[derive(Debug, Clone, Copy)]
pub enum ColorSpace {
    Srgb,
    AdobeRgb,
    DisplayP3,
    Auto,
}

impl FormatOptimization {
    /// Get optimization settings for a specific format and use case
    pub fn for_format(format: ImageFormat, web_optimized: bool) -> Self {
        match format {
            ImageFormat::Jpeg => Self {
                progressive: web_optimized,
                optimize_for_web: web_optimized,
                preserve_metadata: !web_optimized,
                color_space: if web_optimized { ColorSpace::Srgb } else { ColorSpace::Auto },
            },
            ImageFormat::Png => Self {
                progressive: false,
                optimize_for_web: web_optimized,
                preserve_metadata: !web_optimized,
                color_space: ColorSpace::Srgb,
            },
            ImageFormat::WebP => Self {
                progressive: false,
                optimize_for_web: true,
                preserve_metadata: !web_optimized,
                color_space: ColorSpace::Srgb,
            },
            _ => Self {
                progressive: false,
                optimize_for_web: web_optimized,
                preserve_metadata: !web_optimized,
                color_space: ColorSpace::Auto,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_format_detection_from_path() {
        assert_eq!(
            detect_format_from_path(Path::new("test.jpg")).unwrap(),
            ImageFormat::Jpeg
        );
        assert_eq!(
            detect_format_from_path(Path::new("test.PNG")).unwrap(),
            ImageFormat::Png
        );
        assert_eq!(
            detect_format_from_path(Path::new("test.webp")).unwrap(),
            ImageFormat::WebP
        );
    }

    #[test]
    fn test_format_detection_from_header() {
        // JPEG header
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01];
        assert_eq!(
            detect_format_from_header(&jpeg_header).unwrap(),
            ImageFormat::Jpeg
        );

        // PNG header
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D];
        assert_eq!(
            detect_format_from_header(&png_header).unwrap(),
            ImageFormat::Png
        );

        // WebP header
        let webp_header = b"RIFF\x00\x00\x00\x00WEBP";
        assert_eq!(
            detect_format_from_header(webp_header).unwrap(),
            ImageFormat::WebP
        );
    }

    #[test]
    fn test_supported_formats() {
        assert!(is_supported_input_format("jpg"));
        assert!(is_supported_input_format("PNG"));
        assert!(!is_supported_input_format("xyz"));

        assert!(is_supported_output_format("webp"));
        assert!(!is_supported_output_format("raw"));
    }

    #[test]
    fn test_quality_optimization() {
        assert_eq!(get_optimal_quality(ImageFormat::Jpeg, 85), 85);
        assert_eq!(get_optimal_quality(ImageFormat::Png, 85), 15); // Inverse for compression
        assert_eq!(get_optimal_quality(ImageFormat::Gif, 85), 100); // Fixed
    }

    #[test]
    fn test_size_multipliers() {
        // JPEG should have reasonable compression ratios
        assert!(get_size_multiplier(ImageFormat::Jpeg, 80) > 0.1);
        assert!(get_size_multiplier(ImageFormat::Jpeg, 80) < 0.5);
        
        // WebP should be more efficient than JPEG
        assert!(
            get_size_multiplier(ImageFormat::WebP, 80) < 
            get_size_multiplier(ImageFormat::Jpeg, 80)
        );
        
        // BMP should be largest
        assert!(get_size_multiplier(ImageFormat::Bmp, 100) > 1.0);
    }

    #[test]
    fn test_format_optimization() {
        let jpeg_web = FormatOptimization::for_format(ImageFormat::Jpeg, true);
        assert!(jpeg_web.progressive);
        assert!(jpeg_web.optimize_for_web);

        let jpeg_archive = FormatOptimization::for_format(ImageFormat::Jpeg, false);
        assert!(!jpeg_archive.progressive);
        assert!(jpeg_archive.preserve_metadata);
    }
}