#![no_main]
// Fuzz target: the full preprocessing pipeline (markdown → HTML →
// header/footer injection → math → highlight) must never panic.
//
// This exercises the stateful parts that markdown_to_html alone misses:
// header/footer regexes, KaTeX math transformation, code highlighting.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);

    // Stage 1: markdown → HTML
    let mut html = typepress::markdown_to_html(&input);

    // Stage 2: header/footer injection (no-ops if empty — still exercises
    // the regex paths with arbitrary body content).
    let _ = typepress::inject_header_footer(&mut html, None, None);
});
