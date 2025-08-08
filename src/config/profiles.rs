//! Processing profiles for different use cases

use serde::{Deserialize, Serialize};
use crate::config::{ResizeMode, ImageFormat};
use crate::error::{Result, FastResizeError};

/// A processing profile defines how images should be resized
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingProfile {
    /// How to resize the image
    pub resize_mode: ResizeMode,
    
    /// Output quality (1-100)
    pub quality: u8,
    
    /// Output format (None = keep original)
    pub format: Option<ImageFormat>,
    
    /// File naming configuration
    pub naming: NamingConfig,
}

impl ProcessingProfile {
    /// Create a new profile with scale factor
    pub fn scale(factor: f32) -> Self {
        Self {
            resize_mode: ResizeMode::Scale { factor },
            quality: 90,
            format: None,
            naming: NamingConfig::default(),
        }
    }
    
    /// Create a new profile with target width
    pub fn width(width: u32) -> Self {
        Self {
            resize_mode: ResizeMode::Width { width },
            quality: 90,
            format: None,
            naming: NamingConfig::default(),
        }
    }
    
    /// Create a new profile with target height
    pub fn height(height: u32) -> Self {
        Self {
            resize_mode: ResizeMode::Height { height },
            quality: 90,
            format: None,
            naming: NamingConfig::default(),
        }
    }
    
    /// Create a new profile that fits within dimensions
    pub fn fit(width: u32, height: u32) -> Self {
        Self {
            resize_mode: ResizeMode::Fit { width, height },
            quality: 90,
            format: None,
            naming: NamingConfig::default(),
        }
    }
    
    /// Create a new profile that fills dimensions
    pub fn fill(width: u32, height: u32) -> Self {
        Self {
            resize_mode: ResizeMode::Fill { width, height },
            quality: 90,
            format: None,
            naming: NamingConfig::default(),
        }
    }
    
    /// Set the output quality
    pub fn quality(mut self, quality: u8) -> Self {
        self.quality = quality;
        self
    }
    
    /// Set the output format
    pub fn format(mut self, format: ImageFormat) -> Self {
        self.format = Some(format);
        self
    }
    
    /// Set the naming configuration
    pub fn naming(mut self, naming: NamingConfig) -> Self {
        self.naming = naming;
        self
    }
    
    /// Validate the profile configuration
    pub fn validate(&self) -> Result<()> {
        // Validate quality
        if self.quality == 0 || self.quality > 100 {
            return Err(FastResizeError::invalid_parameters(
                format!("Quality must be between 1-100, got {}", self.quality)
            ));
        }
        
        // Validate resize mode
        match &self.resize_mode {
            ResizeMode::Scale { factor } => {
                if *factor <= 0.0 || *factor > 10.0 {
                    return Err(FastResizeError::invalid_parameters(
                        format!("Scale factor must be between 0.0-10.0, got {}", factor)
                    ));
                }
            }
            ResizeMode::Width { width } | ResizeMode::Height { height: width } => {
                if *width == 0 || *width > 32768 {
                    return Err(FastResizeError::invalid_parameters(
                        format!("Dimension must be between 1-32768, got {}", width)
                    ));
                }
            }
            ResizeMode::Fit { width, height } | ResizeMode::Fill { width, height } => {
                if *width == 0 || *width > 32768 || *height == 0 || *height > 32768 {
                    return Err(FastResizeError::invalid_parameters(
                        format!("Dimensions must be between 1-32768, got {}x{}", width, height)
                    ));
                }
            }
        }
        
        self.naming.validate()
    }
}

/// File naming configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingConfig {
    /// Prefix to add to filenames
    pub prefix: Option<String>,
    
    /// Suffix to add to filenames (before extension)
    pub suffix: Option<String>,
    
    /// Keep original filenames (ignore prefix/suffix)
    pub keep_original: bool,
    
    /// Output folder organization
    pub folder_organization: FolderOrganization,
}

impl Default for NamingConfig {
    fn default() -> Self {
        Self {
            prefix: None,
            suffix: Some("_resized".to_string()),
            keep_original: false,
            folder_organization: FolderOrganization::Flat,
        }
    }
}

impl NamingConfig {
    /// Generate output filename for a given input
    pub fn generate_filename(&self, input_name: &str, output_format: Option<ImageFormat>) -> String {
        if self.keep_original {
            return input_name.to_string();
        }
        
        // Split filename and extension
        let (name, original_ext) = if let Some(dot_pos) = input_name.rfind('.') {
            (&input_name[..dot_pos], &input_name[dot_pos + 1..])
        } else {
            (input_name, "")
        };
        
        // Determine output extension
        let extension = if let Some(format) = output_format {
            format.extension()
        } else {
            original_ext
        };
        
        // Build new name
        let mut new_name = String::new();
        
        if let Some(prefix) = &self.prefix {
            new_name.push_str(prefix);
        }
        
        new_name.push_str(name);
        
        if let Some(suffix) = &self.suffix {
            new_name.push_str(suffix);
        }
        
        if !extension.is_empty() {
            new_name.push('.');
            new_name.push_str(extension);
        }
        
        new_name
    }
    
    /// Validate naming configuration
    pub fn validate(&self) -> Result<()> {
        // Check for invalid characters in prefix/suffix
        if let Some(prefix) = &self.prefix {
            if prefix.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
                return Err(FastResizeError::invalid_parameters(
                    "Prefix contains invalid filename characters"
                ));
            }
        }
        
        if let Some(suffix) = &self.suffix {
            if suffix.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
                return Err(FastResizeError::invalid_parameters(
                    "Suffix contains invalid filename characters"
                ));
            }
        }
        
        Ok(())
    }
}

/// Output folder organization strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderOrganization {
    /// All files in single output directory
    Flat,
    
    /// Organize by date (YYYY/MM/DD)
    ByDate,
    
    /// Organize by original folder structure
    MirrorStructure,
    
    /// Organize by image dimensions
    ByDimensions,
    
    /// Organize by file size ranges
    ByFileSize,
}

/// Predefined processing profiles for common use cases
pub struct Profiles;

impl Profiles {
    /// Web optimization profile
    pub fn web() -> ProcessingProfile {
        ProcessingProfile {
            resize_mode: ResizeMode::Width { width: 1920 },
            quality: 85,
            format: Some(ImageFormat::WebP),
            naming: NamingConfig {
                suffix: Some("_web".to_string()),
                ..Default::default()
            },
        }
    }
    
    /// Mobile optimization profile
    pub fn mobile() -> ProcessingProfile {
        ProcessingProfile {
            resize_mode: ResizeMode::Width { width: 768 },
            quality: 75,
            format: Some(ImageFormat::WebP),
            naming: NamingConfig {
                suffix: Some("_mobile".to_string()),
                ..Default::default()
            },
        }
    }
    
    /// Thumbnail generation profile
    pub fn thumbnail() -> ProcessingProfile {
        ProcessingProfile {
            resize_mode: ResizeMode::Fit { width: 300, height: 300 },
            quality: 80,
            format: Some(ImageFormat::WebP),
            naming: NamingConfig {
                suffix: Some("_thumb".to_string()),
                ..Default::default()
            },
        }
    }
    
    /// High-quality print profile
    pub fn print() -> ProcessingProfile {
        ProcessingProfile {
            resize_mode: ResizeMode::Width { width: 3000 },
            quality: 95,
            format: None, // Keep original format
            naming: NamingConfig {
                suffix: Some("_print".to_string()),
                ..Default::default()
            },
        }
    }
    
    /// Social media profile (square crop)
    pub fn social() -> ProcessingProfile {
        ProcessingProfile {
            resize_mode: ResizeMode::Fill { width: 1080, height: 1080 },
            quality: 85,
            format: Some(ImageFormat::Jpeg),
            naming: NamingConfig {
                suffix: Some("_social".to_string()),
                ..Default::default()
            },
        }
    }
    
    /// Email attachment profile (small file size)
    pub fn email() -> ProcessingProfile {
        ProcessingProfile {
            resize_mode: ResizeMode::Width { width: 800 },
            quality: 70,
            format: Some(ImageFormat::Jpeg),
            naming: NamingConfig {
                suffix: Some("_email".to_string()),
                ..Default::default()
            },
        }
    }
    
    /// Archive profile (lossless compression)
    pub fn archive() -> ProcessingProfile {
        ProcessingProfile {
            resize_mode: ResizeMode::Scale { factor: 1.0 }, // No resize
            quality: 100,
            format: Some(ImageFormat::Png),
            naming: NamingConfig {
                suffix: Some("_archive".to_string()),
                ..Default::default()
            },
        }
    }
    
    /// Get all predefined profiles
    pub fn all() -> std::collections::HashMap<String, ProcessingProfile> {
        let mut profiles = std::collections::HashMap::new();
        profiles.insert("web".to_string(), Self::web());
        profiles.insert("mobile".to_string(), Self::mobile());
        profiles.insert("thumbnail".to_string(), Self::thumbnail());
        profiles.insert("print".to_string(), Self::print());
        profiles.insert("social".to_string(), Self::social());
        profiles.insert("email".to_string(), Self::email());
        profiles.insert("archive".to_string(), Self::archive());
        profiles
    }
}

/// Configuration builder for fluent API
#[derive(Debug, Clone)]
pub struct ResizeConfig {
    pub mode: ResizeMode,
    pub quality: u8,
    pub format: Option<ImageFormat>,
}

impl ResizeConfig {
    /// Create a new resize configuration
    pub fn new() -> Self {
        Self {
            mode: ResizeMode::Scale { factor: 1.0 },
            quality: 90,
            format: None,
        }
    }
    
    /// Set resize mode
    pub fn mode(mut self, mode: ResizeMode) -> Self {
        self.mode = mode;
        self
    }
    
    /// Set scale factor
    pub fn scale(mut self, factor: f32) -> Self {
        self.mode = ResizeMode::Scale { factor };
        self
    }
    
    /// Set target width
    pub fn width(mut self, width: u32) -> Self {
        self.mode = ResizeMode::Width { width };
        self
    }
    
    /// Set target height  
    pub fn height(mut self, height: u32) -> Self {
        self.mode = ResizeMode::Height { height };
        self
    }
    
    /// Fit within dimensions
    pub fn fit(mut self, width: u32, height: u32) -> Self {
        self.mode = ResizeMode::Fit { width, height };
        self
    }
    
    /// Fill dimensions
    pub fn fill(mut self, width: u32, height: u32) -> Self {
        self.mode = ResizeMode::Fill { width, height };
        self
    }
    
    /// Set quality
    pub fn quality(mut self, quality: u8) -> Self {
        self.quality = quality;
        self
    }
    
    /// Set output format
    pub fn format(mut self, format: ImageFormat) -> Self {
        self.format = Some(format);
        self
    }
}

impl Default for ResizeConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_creation() {
        let profile = ProcessingProfile::width(1920)
            .quality(85)
            .format(ImageFormat::WebP);
            
        assert!(matches!(profile.resize_mode, ResizeMode::Width { width: 1920 }));
        assert_eq!(profile.quality, 85);
        assert_eq!(profile.format, Some(ImageFormat::WebP));
    }

    #[test]
    fn test_profile_validation() {
        let valid_profile = ProcessingProfile::width(1920);
        assert!(valid_profile.validate().is_ok());
        
        let invalid_profile = ProcessingProfile {
            resize_mode: ResizeMode::Scale { factor: 0.0 },
            quality: 101,
            format: None,
            naming: NamingConfig::default(),
        };
        assert!(invalid_profile.validate().is_err());
    }

    #[test]
    fn test_naming_config() {
        let naming = NamingConfig {
            prefix: Some("web_".to_string()),
            suffix: Some("_resized".to_string()),
            keep_original: false,
            folder_organization: FolderOrganization::Flat,
        };
        
        let filename = naming.generate_filename("photo.jpg", Some(ImageFormat::WebP));
        assert_eq!(filename, "web_photo_resized.webp");
    }

    #[test]
    fn test_predefined_profiles() {
        let web_profile = Profiles::web();
        assert!(web_profile.validate().is_ok());
        
        let all_profiles = Profiles::all();
        assert!(all_profiles.len() >= 7);
        
        for profile in all_profiles.values() {
            assert!(profile.validate().is_ok());
        }
    }

    #[test]
    fn test_resize_config_builder() {
        let config = ResizeConfig::new()
            .width(1920)
            .quality(85)
            .format(ImageFormat::WebP);
            
        assert!(matches!(config.mode, ResizeMode::Width { width: 1920 }));
        assert_eq!(config.quality, 85);
        assert_eq!(config.format, Some(ImageFormat::WebP));
    }

    #[test]
    fn test_filename_generation() {
        let naming = NamingConfig::default();
        
        // Test with format conversion
        let filename = naming.generate_filename("test.jpg", Some(ImageFormat::WebP));
        assert_eq!(filename, "test_resized.webp");
        
        // Test keeping original
        let original_naming = NamingConfig {
            keep_original: true,
            ..Default::default()
        };
        let filename = original_naming.generate_filename("test.jpg", Some(ImageFormat::WebP));
        assert_eq!(filename, "test.jpg");
    }
}