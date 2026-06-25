use anyhow::{Context, Result};
use clap::Parser;
use fulgur::config::{Margin, PageSize};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "typepress",
    version,
    about = "Pure Rust HTML/CSS → PDF engine\n\nwkhtmltopdf compatible — use as a drop-in replacement."
)]
pub struct Cli {
    /// Input HTML file (omit for --stdin)
    pub input: Option<PathBuf>,
    /// Read HTML from stdin
    #[arg(long)]
    pub stdin: bool,
    /// Output PDF file path (use "-" for stdout). Required in CLI mode, optional with --config.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    // Input format
    #[arg(long = "from", default_value = "html")]
    pub from: String,
    // Output format
    #[arg(long = "format", short = 'F', default_value = "pdf")]
    pub format: String,
    #[arg(long, default_value = "2.0")]
    pub scale: f32,
    // Config
    #[arg(short = 'c', long)]
    pub config: Option<PathBuf>,
    // Page: size
    #[arg(short = 's', long, alias = "page-size")]
    pub size: Option<String>,
    #[arg(long = "page-width", conflicts_with = "size")]
    pub page_width: Option<String>,
    #[arg(long = "page-height", conflicts_with = "size")]
    pub page_height: Option<String>,
    #[arg(short = 'l', long)]
    pub landscape: bool,
    #[arg(short = 'O', long)]
    pub orientation: Option<String>,
    // Page: margins (wkhtmltopdf compat)
    #[arg(long = "margin")]
    pub margin: Option<String>,
    #[arg(short = 'T', long = "margin-top")]
    pub margin_top: Option<String>,
    #[arg(short = 'B', long = "margin-bottom")]
    pub margin_bottom: Option<String>,
    #[arg(short = 'L', long = "margin-left")]
    pub margin_left: Option<String>,
    #[arg(short = 'R', long = "margin-right")]
    pub margin_right: Option<String>,
    // Zoom
    #[arg(long = "zoom", default_value = "1.0")]
    pub zoom: f32,
    #[arg(long)]
    pub fit: bool,
    // Metadata
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long = "author")]
    pub authors: Vec<String>,
    #[arg(long)]
    pub language: Option<String>,
    // Assets
    #[arg(long = "font", short = 'f')]
    pub fonts: Vec<PathBuf>,
    #[arg(long = "css", alias = "user-style-sheet")]
    pub css_files: Vec<PathBuf>,
    // Headers & Footers
    #[arg(long = "header", alias = "header-html")]
    pub header: Option<String>,
    #[arg(long = "footer", alias = "footer-html")]
    pub footer: Option<String>,
    // wkhtmltopdf compat no-ops
    #[arg(long, hide = true)]
    pub encoding: Option<String>,
    #[arg(long = "no-outline")]
    pub no_outline: bool,
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,
    // Math
    #[arg(long)]
    pub math: bool,
    #[arg(long = "math-dir")]
    pub math_dir: Option<PathBuf>,
    // PDF features
    #[arg(long)]
    pub bookmarks: bool,
    #[arg(long)]
    pub tagged: bool,
    #[arg(long = "pdf-ua")]
    pub pdf_ua: bool,
    #[arg(long = "no-system-fonts")]
    pub no_system_fonts: bool,
}

impl Cli {
    pub fn resolve_margin(&self) -> Option<Margin> {
        let has_side = self.margin_top.is_some()
            || self.margin_bottom.is_some()
            || self.margin_left.is_some()
            || self.margin_right.is_some();
        if has_side {
            let top = self
                .margin_top
                .as_deref()
                .and_then(|s| parse_length_mm(s).ok())
                .unwrap_or(20.0);
            let bottom = self
                .margin_bottom
                .as_deref()
                .and_then(|s| parse_length_mm(s).ok())
                .unwrap_or(20.0);
            let left = self
                .margin_left
                .as_deref()
                .and_then(|s| parse_length_mm(s).ok())
                .unwrap_or(10.0);
            let right = self
                .margin_right
                .as_deref()
                .and_then(|s| parse_length_mm(s).ok())
                .unwrap_or(10.0);
            let to_pt = |mm: f32| mm * 72.0 / 25.4;
            return Some(Margin {
                top: to_pt(top),
                bottom: to_pt(bottom),
                left: to_pt(left),
                right: to_pt(right),
            });
        }
        self.margin.as_deref().map(parse_margin)
    }
    pub fn resolve_landscape(&self) -> bool {
        if let Some(ref o) = self.orientation {
            o.eq_ignore_ascii_case("landscape")
        } else {
            self.landscape
        }
    }
    pub fn resolve_size(&self) -> Option<String> {
        if let (Some(w), Some(h)) = (self.page_width.as_ref(), self.page_height.as_ref()) {
            Some(format!("{} {}", w, h))
        } else {
            self.size.clone()
        }
    }
}

pub fn parse_page_size(s: &str) -> PageSize {
    match s.to_uppercase().as_str() {
        "A0" => PageSize::custom(841.0, 1189.0),
        "A1" => PageSize::custom(594.0, 841.0),
        "A2" => PageSize::custom(420.0, 594.0),
        "A3" => PageSize::A3,
        "A4" => PageSize::A4,
        "A5" => PageSize::custom(148.0, 210.0),
        "A6" => PageSize::custom(105.0, 148.0),
        "LETTER" => PageSize::LETTER,
        "LEGAL" => PageSize::custom(215.9, 355.6),
        "TABLOID" => PageSize::custom(279.4, 431.8),
        "EXECUTIVE" => PageSize::custom(184.15, 266.7),
        _ => {
            // Try custom WxH in mm: "594x420"
            if let Some((w, h)) = s.split_once('x')
                && let (Ok(w), Ok(h)) = (w.trim().parse::<f32>(), h.trim().parse::<f32>())
                && w > 0.0
                && h > 0.0
            {
                return PageSize::custom(w, h);
            }
            eprintln!("Unknown page size '{s}', defaulting to A4");
            PageSize::A4
        }
    }
}

pub fn page_size_mm(name: &str) -> Option<(f64, f64)> {
    match name.to_uppercase().as_str() {
        "A0" => Some((841.0, 1189.0)),
        "A1" => Some((594.0, 841.0)),
        "A2" => Some((420.0, 594.0)),
        "A3" => Some((297.0, 420.0)),
        "A4" => Some((210.0, 297.0)),
        "A5" => Some((148.0, 210.0)),
        "A6" => Some((105.0, 148.0)),
        "LETTER" => Some((215.9, 279.4)),
        "LEGAL" => Some((215.9, 355.6)),
        "TABLOID" => Some((279.4, 431.8)),
        "EXECUTIVE" => Some((184.15, 266.7)),
        s if s.contains('x') => {
            let parts: Vec<&str> = s.split('x').collect();
            if parts.len() >= 2 {
                let w: f64 = parts[0].trim().parse().ok()?;
                let h: f64 = parts[1].trim().parse().ok()?;
                Some((w, h))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn parse_margin(s: &str) -> Margin {
    let values: Vec<std::result::Result<f32, _>> =
        s.split_whitespace().map(parse_length_mm).collect();
    if values.is_empty() || values.iter().all(|v| v.is_err()) {
        return Margin::default();
    }
    let values: Vec<f32> = values.into_iter().map(|v| v.unwrap_or(20.0)).collect();
    let to_pt = |mm: f32| mm * 72.0 / 25.4;
    match values.as_slice() {
        [all] => Margin::uniform(to_pt(*all)),
        [vert, horiz] => Margin::symmetric(to_pt(*vert), to_pt(*horiz)),
        [top, horiz, bottom] => Margin {
            top: to_pt(*top),
            right: to_pt(*horiz),
            bottom: to_pt(*bottom),
            left: to_pt(*horiz),
        },
        [top, right, bottom, left] => Margin {
            top: to_pt(*top),
            right: to_pt(*right),
            bottom: to_pt(*bottom),
            left: to_pt(*left),
        },
        _ => Margin::default(),
    }
}

pub fn parse_length_mm(s: &str) -> std::result::Result<f32, &'static str> {
    let s = s.trim();
    if let Some(val) = s.strip_suffix("mm") {
        return val.trim().parse::<f32>().map_err(|_| "invalid mm");
    }
    if let Some(val) = s.strip_suffix("cm") {
        return val
            .trim()
            .parse::<f32>()
            .map(|v| v * 10.0)
            .map_err(|_| "invalid cm");
    }
    if let Some(val) = s.strip_suffix("in") {
        return val
            .trim()
            .parse::<f32>()
            .map(|v| v * 25.4)
            .map_err(|_| "invalid in");
    }
    if let Some(val) = s.strip_suffix("pt") {
        return val
            .trim()
            .parse::<f32>()
            .map(|v| v * 25.4 / 72.0)
            .map_err(|_| "invalid pt");
    }
    if let Some(val) = s.strip_suffix("px") {
        return val
            .trim()
            .parse::<f32>()
            .map(|v| v * 25.4 / 96.0)
            .map_err(|_| "invalid px");
    }
    // Plain number → treat as mm
    s.parse::<f32>().map_err(|_| "invalid number")
}

pub(crate) const ESCAPED_PLACEHOLDER: &str = "\x00TXP_ESC_DOLLAR\x00";

pub fn read_input(input: Option<&PathBuf>, stdin: bool) -> Result<String> {
    if stdin {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        Ok(buf)
    } else if let Some(path) = input {
        Ok(std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?)
    } else {
        anyhow::bail!("provide an input HTML file or use --stdin")
    }
}
