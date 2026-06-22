// TypePress pipeline configuration — YAML-driven document workflow.
//
// Usage:
//   typepress render                           # auto-detect typepress.yaml
//   typepress render -c mydoc.yaml             # explicit config
//   typepress render input.md -o out.pdf       # CLI mode (no config)
//
// CLI args override YAML values when both are present.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TypePressConfig {
    /// Input file path (md or html)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<PathBuf>,

    /// Input format: md or html (auto-detected from extension if omitted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,

    /// Output configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputConfig>,

    /// Page configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<PageConfig>,

    /// Font files to bundle
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<PathBuf>,

    /// CSS files to include
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub css: Vec<PathBuf>,

    /// Math rendering config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub math: Option<MathConfig>,

    /// Enable Mermaid diagram rendering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mermaid: Option<bool>,

    /// Header text (top-center, every page)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,

    /// Footer text (bottom-center, every page)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<String>,

    /// PNG scale factor (default: 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,

    /// Document metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataConfig>,

    /// PDF features
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf: Option<PdfConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// PDF output path (omit to skip PDF)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf: Option<PathBuf>,

    /// SVG output path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg: Option<PathBuf>,

    /// PNG output path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageConfig {
    /// Page size: A4, Letter, A3, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    /// Landscape orientation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landscape: Option<bool>,

    /// Page margins in mm
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MathConfig {
    /// Enable math rendering (auto-detects KaTeX fonts)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Explicit KaTeX font directory (overrides auto-detection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_dir: Option<PathBuf>,
}

fn default_true() -> bool { true }

impl Default for MathConfig {
    fn default() -> Self {
        Self { enabled: true, font_dir: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub author: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PdfConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bookmarks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagged: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf_ua: Option<bool>,
}

impl TypePressConfig {
    /// Load config from a YAML file.
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Auto-detect typepress.yaml in current directory or parents.
    pub fn auto_detect() -> Option<(Self, PathBuf)> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let candidate = dir.join("typepress.yaml");
            if candidate.exists() {
                if let Ok(config) = Self::from_file(&candidate) {
                    return Some((config, candidate));
                }
            }
            if !dir.pop() {
                break;
            }
        }
        None
    }
}
