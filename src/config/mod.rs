//! Configuration management for FastResize

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::error::{Result, FastResizeError};

pub mod profiles;
pub use profiles::*;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Processing profiles for different use cases
    pub profiles: HashMap<String, ProcessingProfile>,
    
    /// Global processing settings
    pub processing: ProcessingConfig,
    
    /// Automation and monitoring settings
    pub automation: AutomationConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        
        // Default web profile
        profiles.insert("web".to_string(), ProcessingProfile {
            resize_mode: ResizeMode::Width { width: 1920 },
            quality: 85,
            format: Some(ImageFormat::WebP),
            naming: NamingConfig::default(),
        });
        
        // Default thumbnail profile  
        profiles.insert("thumbnail".to_string(), ProcessingProfile {
            resize_mode: ResizeMode::Fit { width: 300, height: 300 },
            quality: 80,
            format: Some(ImageFormat::WebP),
            naming: NamingConfig {
                suffix: Some("_thumb".to_string()),
                ..Default::default()
            },
        });

        Self {
            profiles,
            processing: ProcessingConfig::default(),
            automation: AutomationConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

/// Global processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    /// Number of worker threads (None = auto-detect)
    pub threads: Option<usize>,
    
    /// Maximum memory usage in bytes
    pub memory_limit: Option<u64>,
    
    /// Enable recursive directory processing
    pub recursive: bool,
    
    /// Maximum file size to process (in bytes)
    pub max_file_size: u64,
    
    /// Maximum image dimensions (width * height)
    pub max_image_pixels: u64,
    
    /// Processing timeout per file (in seconds)
    pub timeout_seconds: u64,
    
    /// Batch size for parallel processing
    pub batch_size: usize,
    
    /// Enable memory-mapped file processing for large files
    pub enable_mmap: bool,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            threads: None, // Auto-detect
            memory_limit: Some(4 * 1024 * 1024 * 1024), // 4GB
            recursive: false,
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_image_pixels: 100_000_000, // 100 megapixels
            timeout_seconds: 30,
            batch_size: 50,
            enable_mmap: true,
        }
    }
}

/// Automation and monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationConfig {
    /// File watching configuration
    pub watch_folders: Vec<WatchFolder>,
    
    /// Watch interval in milliseconds
    pub watch_interval: u64,
    
    /// Error retry attempts
    pub error_retry: u32,
    
    /// Enable progress reporting
    pub progress_reporting: bool,
    
    /// Enable JSON output for automation
    pub json_output: bool,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            watch_folders: Vec::new(),
            watch_interval: 1000, // 1 second
            error_retry: 3,
            progress_reporting: true,
            json_output: false,
        }
    }
}

/// Watch folder configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchFolder {
    /// Path to watch
    pub path: PathBuf,
    
    /// Processing profile to use
    pub profile: String,
    
    /// Output directory
    pub output: PathBuf,
    
    /// Process subdirectories recursively
    pub recursive: bool,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,
    
    /// Enable JSON logging
    pub json_format: bool,
    
    /// Log file path (None = stdout)
    pub file: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            json_format: false,
            file: None,
        }
    }
}

/// Resize mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResizeMode {
    /// Scale by factor
    #[serde(rename = "scale")]
    Scale { factor: f32 },
    
    /// Resize to specific width, maintain aspect ratio
    #[serde(rename = "width")]
    Width { width: u32 },
    
    /// Resize to specific height, maintain aspect ratio
    #[serde(rename = "height")]
    Height { height: u32 },
    
    /// Fit within dimensions (letterbox/pillarbox)
    #[serde(rename = "fit")]
    Fit { width: u32, height: u32 },
    
    /// Fill dimensions (crop if necessary)
    #[serde(rename = "fill")]
    Fill { width: u32, height: u32 },
}

/// Supported image formats
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebP,
    Gif,
    Tiff,
    Bmp,
}

impl ImageFormat {
    /// Get file extension for this format
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
            Self::Gif => "gif",
            Self::Tiff => "tiff",
            Self::Bmp => "bmp",
        }
    }

    /// Get MIME type for this format
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::WebP => "image/webp", 
            Self::Gif => "image/gif",
            Self::Tiff => "image/tiff",
            Self::Bmp => "image/bmp",
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| FastResizeError::config(
                format!("Failed to read config file {:?}: {}", path.as_ref(), e)
            ))?;
        
        let extension = path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        
        match extension.to_lowercase().as_str() {
            "toml" => toml::from_str(&content).map_err(Into::into),
            "yaml" | "yml" => serde_yaml::from_str(&content).map_err(Into::into),
            _ => Err(FastResizeError::config(
                "Unsupported config file format. Use .toml or .yaml"
            )),
        }
    }
    
    /// Save configuration to file
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let extension = path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
            
        let content = match extension.to_lowercase().as_str() {
            "toml" => toml::to_string_pretty(self)
                .map_err(|e| FastResizeError::config(format!("TOML serialization failed: {}", e)))?,
            "yaml" | "yml" => serde_yaml::to_string(self)
                .map_err(|e| FastResizeError::config(format!("YAML serialization failed: {}", e)))?,
            _ => return Err(FastResizeError::config(
                "Unsupported config file format. Use .toml or .yaml"
            )),
        };
        
        std::fs::write(&path, content)
            .map_err(|e| FastResizeError::config(
                format!("Failed to write config file {:?}: {}", path.as_ref(), e)
            ))?;
            
        Ok(())
    }
    
    /// Get a processing profile by name
    pub fn get_profile(&self, name: &str) -> Result<&ProcessingProfile> {
        self.profiles.get(name)
            .ok_or_else(|| FastResizeError::config(
                format!("Profile '{}' not found. Available profiles: {:?}", 
                       name, self.profiles.keys().collect::<Vec<_>>())
            ))
    }
    
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate profiles
        for (name, profile) in &self.profiles {
            profile.validate()
                .map_err(|e| FastResizeError::config(
                    format!("Invalid profile '{}': {}", name, e)
                ))?;
        }
        
        // Validate processing settings
        if let Some(threads) = self.processing.threads {
            if threads == 0 {
                return Err(FastResizeError::config(
                    "Thread count must be greater than 0"
                ));
            }
        }
        
        if self.processing.batch_size == 0 {
            return Err(FastResizeError::config(
                "Batch size must be greater than 0"
            ));
        }
        
        // Validate watch folders
        for watch_folder in &self.automation.watch_folders {
            if !self.profiles.contains_key(&watch_folder.profile) {
                return Err(FastResizeError::config(
                    format!("Watch folder references unknown profile: {}", watch_folder.profile)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Merge with another configuration (other takes precedence)
    pub fn merge(mut self, other: Config) -> Self {
        // Merge profiles (other wins on conflicts)
        self.profiles.extend(other.profiles);
        
        // Replace processing config if provided
        if other.processing.threads.is_some() {
            self.processing.threads = other.processing.threads;
        }
        if other.processing.memory_limit.is_some() {
            self.processing.memory_limit = other.processing.memory_limit;
        }
        
        // Extend automation config
        self.automation.watch_folders.extend(other.automation.watch_folders);
        
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.profiles.contains_key("web"));
        assert!(config.profiles.contains_key("thumbnail"));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        
        // Test TOML
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.profiles.len(), parsed.profiles.len());
        
        // Test YAML
        let yaml_str = serde_yaml::to_string(&config).unwrap();
        let parsed: Config = serde_yaml::from_str(&yaml_str).unwrap();
        assert_eq!(config.profiles.len(), parsed.profiles.len());
    }

    #[test]
    fn test_config_file_io() {
        let config = Config::default();
        
        // Test TOML file
        let toml_file = NamedTempFile::new().unwrap();
        let toml_path = toml_file.path().with_extension("toml");
        config.to_file(&toml_path).unwrap();
        let loaded = Config::from_file(&toml_path).unwrap();
        assert!(loaded.validate().is_ok());
        
        // Test YAML file
        let yaml_file = NamedTempFile::new().unwrap();
        let yaml_path = yaml_file.path().with_extension("yaml");
        config.to_file(&yaml_path).unwrap();
        let loaded = Config::from_file(&yaml_path).unwrap();
        assert!(loaded.validate().is_ok());
    }

    #[test]
    fn test_image_format_properties() {
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
        assert_eq!(ImageFormat::WebP.mime_type(), "image/webp");
    }

    #[test]
    fn test_profile_lookup() {
        let config = Config::default();
        assert!(config.get_profile("web").is_ok());
        assert!(config.get_profile("nonexistent").is_err());
    }
}