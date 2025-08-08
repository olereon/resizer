//! Image and file validation utilities

use std::path::Path;
use tokio::fs;
use tracing::{debug, warn};

use crate::error::{Result, FastResizeError};
use crate::processing::formats::{detect_format_from_path, detect_format_from_header, is_supported_input_format};

/// Image validator for checking file integrity and compatibility
pub struct ImageValidator {
    max_file_size: u64,
    max_image_pixels: u64,
    max_dimension: u32,
}

impl ImageValidator {
    /// Create a new image validator with default limits
    pub fn new() -> Self {
        Self {
            max_file_size: 500 * 1024 * 1024,    // 500MB
            max_image_pixels: 500_000_000,       // 500 megapixels
            max_dimension: 32768,                // 32K pixels per dimension
        }
    }

    /// Create a validator with custom limits
    pub fn with_limits(
        max_file_size_mb: u64,
        max_megapixels: u64,
        max_dimension: u32,
    ) -> Self {
        Self {
            max_file_size: max_file_size_mb * 1024 * 1024,
            max_image_pixels: max_megapixels * 1_000_000,
            max_dimension,
        }
    }

    /// Validate a file for processing
    pub async fn validate_file<P: AsRef<Path>>(&self, path: P) -> Result<ValidationResult> {
        let path = path.as_ref();
        debug!("Validating file: {:?}", path);

        // Check if file exists and is readable
        let metadata = fs::metadata(path).await
            .map_err(|e| FastResizeError::validation(
                format!("Cannot access file: {}", e),
                Some(path.to_path_buf()),
            ))?;

        if !metadata.is_file() {
            return Err(FastResizeError::validation(
                "Path is not a regular file".to_string(),
                Some(path.to_path_buf()),
            ));
        }

        let file_size = metadata.len();

        // Check file size limits
        if file_size == 0 {
            return Err(FastResizeError::validation(
                "File is empty".to_string(),
                Some(path.to_path_buf()),
            ));
        }

        if file_size > self.max_file_size {
            return Err(FastResizeError::file_too_large(
                file_size,
                self.max_file_size,
                path.to_path_buf(),
            ));
        }

        // Check file extension
        let format = detect_format_from_path(path)?;
        
        let extension = path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        if !is_supported_input_format(extension) {
            return Err(FastResizeError::unsupported_format(
                extension.to_string(),
                Some(path.to_path_buf()),
            ));
        }

        // Validate file header (magic bytes)
        let header_validation = self.validate_file_header(path).await?;

        // Quick image dimension check (if possible without full load)
        let dimension_check = self.quick_dimension_check(path, file_size).await?;

        let result = ValidationResult {
            path: path.to_path_buf(),
            file_size,
            format,
            is_valid: true,
            header_valid: header_validation.header_valid,
            estimated_dimensions: dimension_check.dimensions,
            estimated_pixels: dimension_check.pixels,
            warnings: vec![],
            errors: vec![],
        };

        debug!("Validation completed: {:?}", result);
        Ok(result)
    }

    /// Validate file header (magic bytes)
    async fn validate_file_header<P: AsRef<Path>>(&self, path: P) -> Result<HeaderValidation> {
        let path = path.as_ref();
        
        // Read first 32 bytes for format detection
        let mut file = fs::File::open(path).await
            .map_err(|e| FastResizeError::validation(
                format!("Cannot open file for header validation: {}", e),
                Some(path.to_path_buf()),
            ))?;

        use tokio::io::AsyncReadExt;
        let mut header = vec![0u8; 32];
        let bytes_read = file.read(&mut header).await
            .map_err(|e| FastResizeError::validation(
                format!("Cannot read file header: {}", e),
                Some(path.to_path_buf()),
            ))?;

        if bytes_read < 8 {
            return Err(FastResizeError::validation(
                "File too small to contain valid image header".to_string(),
                Some(path.to_path_buf()),
            ));
        }

        // Detect format from header
        let header_format = detect_format_from_header(&header[..bytes_read]);
        let path_format = detect_format_from_path(path);

        let header_valid = match (header_format, path_format) {
            (Ok(header_fmt), Ok(path_fmt)) => {
                if header_fmt as u8 != path_fmt as u8 {
                    warn!(
                        "Format mismatch: header indicates {:?}, extension indicates {:?}",
                        header_fmt, path_fmt
                    );
                    false
                } else {
                    true
                }
            }
            (Ok(_), Err(_)) => {
                // Header is valid but extension is unknown/unsupported
                true
            }
            (Err(_), Ok(_)) => {
                // Extension suggests valid format but header is invalid
                false
            }
            (Err(_), Err(_)) => {
                // Both invalid
                false
            }
        };

        Ok(HeaderValidation { header_valid })
    }

    /// Quick dimension check without full image decode
    async fn quick_dimension_check<P: AsRef<Path>>(
        &self,
        path: P,
        file_size: u64,
    ) -> Result<DimensionCheck> {
        let path = path.as_ref();
        
        // For now, we'll use heuristics based on file size
        // In the future, we could implement format-specific dimension reading
        let estimated_pixels = match detect_format_from_path(path)? {
            crate::config::ImageFormat::Jpeg => {
                // JPEG: estimate based on compression ratio
                // Assume average of 2 bytes per pixel for moderate quality
                file_size * 2 / 3
            }
            crate::config::ImageFormat::Png => {
                // PNG: less compression, estimate 3-4 bytes per pixel
                file_size / 4
            }
            crate::config::ImageFormat::WebP => {
                // WebP: similar to JPEG but more efficient
                file_size / 2
            }
            crate::config::ImageFormat::Bmp => {
                // BMP: uncompressed, 3-4 bytes per pixel
                file_size / 4
            }
            _ => {
                // Conservative estimate
                file_size / 3
            }
        };

        // Estimate square dimensions (worst case for memory usage)
        let estimated_side = (estimated_pixels as f64).sqrt() as u32;
        let estimated_dimensions = Some((estimated_side, estimated_side));

        // Check against limits
        if estimated_pixels > self.max_image_pixels {
            return Err(FastResizeError::image_too_large(
                estimated_side,
                estimated_side,
                self.max_image_pixels,
                Some(path.to_path_buf()),
            ));
        }

        if estimated_side > self.max_dimension {
            return Err(FastResizeError::image_too_large(
                estimated_side,
                estimated_side,
                self.max_dimension as u64,
                Some(path.to_path_buf()),
            ));
        }

        Ok(DimensionCheck {
            dimensions: estimated_dimensions,
            pixels: estimated_pixels,
        })
    }

    /// Validate image dimensions from loaded image
    pub fn validate_dimensions(&self, width: u32, height: u32, path: Option<&Path>) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(FastResizeError::validation(
                "Image has zero width or height".to_string(),
                path.map(|p| p.to_path_buf()),
            ));
        }

        if width > self.max_dimension || height > self.max_dimension {
            return Err(FastResizeError::image_too_large(
                width,
                height,
                self.max_dimension as u64,
                path.map(|p| p.to_path_buf()),
            ));
        }

        let total_pixels = (width as u64) * (height as u64);
        if total_pixels > self.max_image_pixels {
            return Err(FastResizeError::image_too_large(
                width,
                height,
                self.max_image_pixels,
                path.map(|p| p.to_path_buf()),
            ));
        }

        Ok(())
    }

    /// Check if a file is likely processable based on quick heuristics
    pub async fn quick_check<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();
        
        // Check extension
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            if !is_supported_input_format(extension) {
                return false;
            }
        } else {
            return false;
        }

        // Check file size
        if let Ok(metadata) = fs::metadata(path).await {
            if metadata.len() == 0 || metadata.len() > self.max_file_size {
                return false;
            }
        } else {
            return false;
        }

        true
    }
}

impl Default for ImageValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of file validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub path: std::path::PathBuf,
    pub file_size: u64,
    pub format: crate::config::ImageFormat,
    pub is_valid: bool,
    pub header_valid: bool,
    pub estimated_dimensions: Option<(u32, u32)>,
    pub estimated_pixels: u64,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug)]
struct HeaderValidation {
    header_valid: bool,
}

#[derive(Debug)]
struct DimensionCheck {
    dimensions: Option<(u32, u32)>,
    pixels: u64,
}

/// Batch validator for processing multiple files
pub struct BatchValidator {
    validator: ImageValidator,
}

impl BatchValidator {
    /// Create a new batch validator
    pub fn new(validator: ImageValidator) -> Self {
        Self { validator }
    }

    /// Validate multiple files concurrently
    pub async fn validate_batch<P: AsRef<Path> + Send + 'static>(
        &self,
        paths: Vec<P>,
    ) -> Vec<Result<ValidationResult>> {
        use futures::future::join_all;

        let validators = paths.into_iter().map(|path| {
            let validator = &self.validator;
            async move { validator.validate_file(path).await }
        });

        join_all(validators).await
    }

    /// Quick filter for potentially valid files
    pub async fn quick_filter<P: AsRef<Path> + Send + Clone + 'static>(
        &self,
        paths: Vec<P>,
    ) -> Vec<P> {
        use futures::future::join_all;

        let checks = paths.iter().cloned().map(|path| {
            let validator = &self.validator;
            async move {
                let is_valid = validator.quick_check(&path).await;
                (path, is_valid)
            }
        });

        let results = join_all(checks).await;
        results.into_iter()
            .filter_map(|(path, is_valid)| if is_valid { Some(path) } else { None })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[tokio::test]
    async fn test_validator_creation() {
        let validator = ImageValidator::new();
        assert_eq!(validator.max_file_size, 500 * 1024 * 1024);
        
        let custom_validator = ImageValidator::with_limits(100, 50, 16384);
        assert_eq!(custom_validator.max_file_size, 100 * 1024 * 1024);
        assert_eq!(custom_validator.max_image_pixels, 50_000_000);
    }

    #[tokio::test]
    async fn test_file_existence_validation() {
        let validator = ImageValidator::new();
        
        // Test non-existent file
        let result = validator.validate_file("nonexistent.jpg").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_file_validation() {
        let validator = ImageValidator::new();
        
        // Create empty temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let result = validator.validate_file(temp_file.path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unsupported_format_validation() {
        let validator = ImageValidator::new();
        
        // Create temporary file with unsupported extension
        let mut temp_file = NamedTempFile::with_suffix(".xyz").unwrap();
        temp_file.write_all(b"dummy content").unwrap();
        
        let result = validator.validate_file(temp_file.path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dimension_validation() {
        let validator = ImageValidator::new();
        
        // Test valid dimensions
        assert!(validator.validate_dimensions(1920, 1080, None).is_ok());
        
        // Test zero dimensions
        assert!(validator.validate_dimensions(0, 1080, None).is_err());
        assert!(validator.validate_dimensions(1920, 0, None).is_err());
        
        // Test oversized dimensions
        assert!(validator.validate_dimensions(50000, 50000, None).is_err());
    }

    #[tokio::test]
    async fn test_quick_check() {
        let validator = ImageValidator::new();
        
        // Should fail for non-existent file
        assert!(!validator.quick_check("nonexistent.jpg").await);
        
        // Create a temporary file with valid extension
        let mut temp_file = NamedTempFile::with_suffix(".jpg").unwrap();
        temp_file.write_all(b"dummy jpeg content").unwrap();
        
        // Should pass quick check (even though content is not valid JPEG)
        assert!(validator.quick_check(temp_file.path()).await);
    }

    #[tokio::test]
    async fn test_header_validation() {
        let validator = ImageValidator::new();
        
        // Create file with JPEG magic bytes
        let mut temp_file = NamedTempFile::with_suffix(".jpg").unwrap();
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        temp_file.write_all(&jpeg_header).unwrap();
        temp_file.write_all(b"dummy content").unwrap();
        
        let header_validation = validator.validate_file_header(temp_file.path()).await;
        assert!(header_validation.is_ok());
    }

    #[tokio::test]
    async fn test_batch_validation() {
        let validator = ImageValidator::new();
        let batch_validator = BatchValidator::new(validator);
        
        // Create some temporary files
        let mut temp_files = Vec::new();
        for i in 0..3 {
            let mut temp_file = NamedTempFile::with_suffix(".jpg").unwrap();
            temp_file.write_all(&format!("content {}", i).as_bytes()).unwrap();
            temp_files.push(temp_file);
        }
        
        let paths: Vec<_> = temp_files.iter().map(|f| f.path()).collect();
        let results = batch_validator.validate_batch(paths.clone()).await;
        
        assert_eq!(results.len(), 3);
        
        // Test quick filter
        let filtered = batch_validator.quick_filter(paths).await;
        assert_eq!(filtered.len(), 3); // All should have valid extensions
    }
}