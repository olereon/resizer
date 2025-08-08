//! Error types and handling for FastResize

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for FastResize operations
pub type Result<T> = std::result::Result<T, FastResizeError>;

/// Main error type for FastResize operations
#[derive(Debug, Error)]
pub enum FastResizeError {
    /// I/O related errors
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Image format or processing errors
    #[error("Image processing error: {0}")]
    ImageError(#[from] image::ImageError),

    /// Configuration errors
    #[error("Configuration error: {message}")]
    ConfigError { message: String },

    /// File format not supported
    #[error("Unsupported image format: {format} (file: {file:?})")]
    UnsupportedFormat { 
        format: String, 
        file: Option<PathBuf> 
    },

    /// Image dimensions too large
    #[error("Image too large: {width}x{height} pixels (limit: {limit} pixels, file: {file:?})")]
    ImageTooLarge {
        width: u32,
        height: u32,
        limit: u64,
        file: Option<PathBuf>,
    },

    /// File size too large
    #[error("File too large: {size} bytes (limit: {limit} bytes, file: {file:?})")]
    FileTooLarge {
        size: u64,
        limit: u64,
        file: PathBuf,
    },

    /// Memory allocation failed
    #[error("Memory allocation failed: {message}")]
    OutOfMemory { message: String },

    /// Processing timeout
    #[error("Processing timeout after {timeout_secs}s (file: {file:?})")]
    Timeout {
        timeout_secs: u64,
        file: Option<PathBuf>,
    },

    /// Invalid resize parameters
    #[error("Invalid resize parameters: {message}")]
    InvalidParameters { message: String },

    /// File validation errors
    #[error("File validation failed: {message} (file: {file:?})")]
    ValidationError {
        message: String,
        file: Option<PathBuf>,
    },

    /// Parallel processing errors
    #[error("Parallel processing error: {message}")]
    ParallelError { message: String },

    /// Watch mode errors
    #[error("File watching error: {0}")]
    WatchError(#[from] notify::Error),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    SerdeError(String),

    /// System resource errors
    #[error("System resource error: {message}")]
    SystemError { message: String },
}

impl FastResizeError {
    /// Create a new configuration error
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::ConfigError {
            message: message.into(),
        }
    }

    /// Create a new unsupported format error
    pub fn unsupported_format<S: Into<String>>(format: S, file: Option<PathBuf>) -> Self {
        Self::UnsupportedFormat {
            format: format.into(),
            file,
        }
    }

    /// Create a new image too large error
    pub fn image_too_large(
        width: u32, 
        height: u32, 
        limit: u64, 
        file: Option<PathBuf>
    ) -> Self {
        Self::ImageTooLarge {
            width,
            height,
            limit,
            file,
        }
    }

    /// Create a new file too large error
    pub fn file_too_large(size: u64, limit: u64, file: PathBuf) -> Self {
        Self::FileTooLarge { size, limit, file }
    }

    /// Create a new out of memory error
    pub fn out_of_memory<S: Into<String>>(message: S) -> Self {
        Self::OutOfMemory {
            message: message.into(),
        }
    }

    /// Create a new timeout error
    pub fn timeout(timeout_secs: u64, file: Option<PathBuf>) -> Self {
        Self::Timeout {
            timeout_secs,
            file,
        }
    }

    /// Create a new invalid parameters error
    pub fn invalid_parameters<S: Into<String>>(message: S) -> Self {
        Self::InvalidParameters {
            message: message.into(),
        }
    }

    /// Create a new validation error
    pub fn validation<S: Into<String>>(message: S, file: Option<PathBuf>) -> Self {
        Self::ValidationError {
            message: message.into(),
            file,
        }
    }

    /// Create a new parallel processing error
    pub fn parallel<S: Into<String>>(message: S) -> Self {
        Self::ParallelError {
            message: message.into(),
        }
    }

    /// Create a new system error
    pub fn system<S: Into<String>>(message: S) -> Self {
        Self::SystemError {
            message: message.into(),
        }
    }

    /// Check if this error is recoverable (processing can continue)
    pub fn is_recoverable(&self) -> bool {
        match self {
            // These errors should stop all processing
            Self::OutOfMemory { .. } 
            | Self::SystemError { .. }
            | Self::ParallelError { .. } => false,

            // These errors affect individual files but processing can continue
            Self::IoError(_)
            | Self::ImageError(_)
            | Self::UnsupportedFormat { .. }
            | Self::ImageTooLarge { .. }
            | Self::FileTooLarge { .. }
            | Self::Timeout { .. }
            | Self::ValidationError { .. } => true,

            // Configuration errors should stop processing  
            Self::ConfigError { .. }
            | Self::InvalidParameters { .. }
            | Self::SerdeError(_) => false,

            // Watch errors are context-dependent
            Self::WatchError(_) => true,
        }
    }

    /// Get the associated file path if available
    pub fn file_path(&self) -> Option<&PathBuf> {
        match self {
            Self::UnsupportedFormat { file, .. }
            | Self::ImageTooLarge { file, .. }
            | Self::Timeout { file, .. }
            | Self::ValidationError { file, .. } => file.as_ref(),
            
            Self::FileTooLarge { file, .. } => Some(file),
            
            _ => None,
        }
    }

    /// Get a user-friendly error message
    pub fn user_message(&self) -> String {
        match self {
            Self::IoError(e) => format!("File system error: {}", e),
            Self::ImageError(e) => format!("Image processing failed: {}", e),
            Self::UnsupportedFormat { format, .. } => {
                format!("Unsupported image format: {}. Supported formats: JPEG, PNG, WebP, GIF, TIFF", format)
            }
            Self::ImageTooLarge { width, height, limit, .. } => {
                format!(
                    "Image is too large ({}x{} = {} pixels). Maximum supported: {} pixels",
                    width, height, (*width as u64) * (*height as u64), limit
                )
            }
            Self::FileTooLarge { size, limit, .. } => {
                format!(
                    "File is too large ({:.2} MB). Maximum supported: {:.2} MB",
                    *size as f64 / 1024.0 / 1024.0,
                    *limit as f64 / 1024.0 / 1024.0
                )
            }
            Self::OutOfMemory { .. } => {
                "Insufficient memory. Try processing fewer files at once or use a smaller scale factor.".to_string()
            }
            Self::Timeout { timeout_secs, .. } => {
                format!("Processing took too long (>{} seconds). Try with a smaller image or reduce quality.", timeout_secs)
            }
            other => other.to_string(),
        }
    }
}

// Convert serde errors to our error type
impl From<toml::de::Error> for FastResizeError {
    fn from(err: toml::de::Error) -> Self {
        Self::SerdeError(format!("TOML parsing error: {}", err))
    }
}

impl From<serde_yaml::Error> for FastResizeError {
    fn from(err: serde_yaml::Error) -> Self {
        Self::SerdeError(format!("YAML parsing error: {}", err))
    }
}

/// Error context extension for adding file path information
pub trait ErrorContext<T> {
    /// Add file context to an error
    fn with_file_context(self, file: PathBuf) -> Result<T>;
}

impl<T, E> ErrorContext<T> for std::result::Result<T, E>
where
    E: Into<FastResizeError>,
{
    fn with_file_context(self, file: PathBuf) -> Result<T> {
        self.map_err(|e| {
            let mut error = e.into();
            
            // Add file context if not already present
            match &mut error {
                FastResizeError::UnsupportedFormat { file: ref mut f, .. }
                | FastResizeError::ImageTooLarge { file: ref mut f, .. }
                | FastResizeError::Timeout { file: ref mut f, .. }
                | FastResizeError::ValidationError { file: ref mut f, .. } => {
                    if f.is_none() {
                        *f = Some(file);
                    }
                }
                _ => {}
            }
            
            error
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = FastResizeError::config("test message");
        assert!(matches!(err, FastResizeError::ConfigError { .. }));
    }

    #[test]
    fn test_recoverable_errors() {
        assert!(FastResizeError::validation("test", None).is_recoverable());
        assert!(!FastResizeError::out_of_memory("test").is_recoverable());
    }

    #[test]
    fn test_user_messages() {
        let err = FastResizeError::unsupported_format("BMP", None);
        let msg = err.user_message();
        assert!(msg.contains("Unsupported image format"));
        assert!(msg.contains("JPEG, PNG, WebP"));
    }

    #[test]
    fn test_file_context() {
        use std::path::Path;
        
        let result: Result<()> = Err(FastResizeError::config("test"));
        let result_with_context = result.with_file_context(Path::new("test.jpg").to_path_buf());
        
        assert!(result_with_context.is_err());
    }
}