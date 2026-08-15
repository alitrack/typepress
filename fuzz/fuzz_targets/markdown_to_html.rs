#![no_main]
// Fuzz target: `markdown_to_html` must never panic on arbitrary input.
//
// This is the first stage of the Markdown→PDF pipeline. A panic here
// (regex, cmark, or span handling) would crash the whole render.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Accept arbitrary bytes; lossy-convert to UTF-8 like real input
    // (read_input reads files, which may not be valid UTF-8).
    let input = String::from_utf8_lossy(data);
    // The public API must not panic on any input.
    let _ = typepress::markdown_to_html(&input);
});
