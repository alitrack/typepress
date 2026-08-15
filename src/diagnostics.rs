// TypePress diagnostics — structured warnings with stable codes.
//
// Industrial-grade reliability: TypePress must never fail silently. Every
// recoverable condition (missing image, failed remote fetch, dropped unsized
// img, renderer parse noise) is accumulated here as a structured warning with
// a stable code, surfaced in `--json` output and, under `--strict`, escalated
// to exit code 2.
//
// Warning codes (stable, documented in README / --help):
//   TP-1001  remote image download failed
//   TP-1002  remote image zero-byte / undecodable
//   TP-1003  asset exceeded --max-asset-bytes / redirect cap
//   TP-1004  remote CSS fetch failed
//   TP-1005  font / emoji download failed or exceeded limit
//   TP-1006  unsized <img> dropped by renderer (auto-size injection failed)
//   TP-1007  constrain_images skipped (no height available)
//   TP-1008  blitz parse noise (aggregate count)
//   TP-1009  multi-page output (try --fit or --page-size A3)
//   TP-1010  no text or images — possible rendering failure

use serde::Serialize;

/// A single structured warning.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Warning {
    pub code: &'static str,
    pub message: String,
}

impl Warning {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Warning {
            code,
            message: message.into(),
        }
    }
}

/// Accumulator for structured warnings. Plain struct threaded through call
/// sites as `&mut Diagnostics` — no globals, no thread_local.
#[derive(Debug, Default, Clone)]
pub struct Diagnostics {
    warnings: Vec<Warning>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Diagnostics::default()
    }

    pub fn push(&mut self, code: &'static str, message: impl Into<String>) {
        self.warnings.push(Warning::new(code, message));
    }

    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    pub fn is_empty(&self) -> bool {
        self.warnings.is_empty()
    }

    pub fn len(&self) -> usize {
        self.warnings.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_collects_warnings() {
        let mut d = Diagnostics::new();
        assert!(d.is_empty());
        d.push("TP-1001", "boom");
        assert_eq!(d.len(), 1);
        assert!(!d.is_empty());
        assert_eq!(d.warnings()[0].code, "TP-1001");
        assert_eq!(d.warnings()[0].message, "boom");
    }

    #[test]
    fn diagnostics_multiple_warnings_preserve_order() {
        let mut d = Diagnostics::new();
        d.push("TP-1001", "first");
        d.push("TP-1002", "second");
        let w = d.warnings();
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].code, "TP-1001");
        assert_eq!(w[1].code, "TP-1002");
    }

    #[test]
    fn warning_serializes_to_json_shape() {
        let w = Warning::new("TP-1001", "failed to download https://x.example/img.png");
        let json = serde_json::to_value(&w).unwrap();
        assert_eq!(json["code"], "TP-1001");
        assert_eq!(
            json["message"],
            "failed to download https://x.example/img.png"
        );
    }
}
