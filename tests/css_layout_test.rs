// TypePress CSS Layout reftests.

#[cfg(test)]
mod css_layout_tests {
    use std::process::Command;
    use typepress::css_layout::process_css_layout;

    const TEST_HTML: &str = include_str!("../templates/agent-knowledge-map.html");

    #[test]
    fn layout_conversion_preserves_all_concepts() {
        let output = process_css_layout(TEST_HTML);
        let concepts = [
            "ReAct", "CoT", "Reflection", "Plan-Execute", "LoopAgent",
            "Tool Calling", "Sandbox", "MCP", "Structured Output",
            "Model Routing", "RAG", "Vector DB", "Embedding", "Chunking",
            "Memory System", "Agent Evaluation", "HITL", "Observability",
            "Guardrails", "A2A Protocol", "Computer Use",
            "Multi-Model Fusion", "Distributed Agents",
        ];
        for c in &concepts {
            assert!(output.contains(c), "Concept '{c}' lost during layout conversion");
        }
    }

    #[test]
    fn layout_conversion_removes_grid_and_gradient() {
        let output = process_css_layout(TEST_HTML);
        assert!(!output.contains("display: grid"), "Grid not converted");
        assert!(!output.contains("linear-gradient"), "Gradient not degraded");
        assert!(output.contains("<table"), "No tables in output");
        assert!(output.matches("<table").count() >= 5, "Too few tables");
    }

    #[test]
    fn layout_conversion_preserves_svg() {
        let html = r#"<div class="main-grid">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
                <rect x="0" y="0" width="100" height="100" fill="red"/>
            </svg>
        </div>"#;
        let output = process_css_layout(html);
        assert!(output.contains("<svg"), "SVG lost");
        assert!(output.contains("<rect"), "SVG child lost");
    }

    #[test]
    fn layout_pdf_page_count_and_validity() {
        let processed = process_css_layout(TEST_HTML);
        let tmp_html = std::env::temp_dir().join("typepress_layout_test.html");
        std::fs::write(&tmp_html, &processed).unwrap();

        let tmp_pdf = std::env::temp_dir().join("typepress_layout_test.pdf");

        let typepress_bin = std::env::current_dir()
            .unwrap()
            .join("target/debug/typepress");

        let output = Command::new(&typepress_bin)
            .arg(&tmp_html)
            .arg("-o")
            .arg(&tmp_pdf)
            .output()
            .expect("Failed to run typepress");

        assert!(
            output.status.success(),
            "typepress failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let pdf_bytes = std::fs::read(&tmp_pdf).unwrap();
        assert!(pdf_bytes.starts_with(b"%PDF-"), "Not a valid PDF");
        assert!(pdf_bytes.len() < 200_000, "PDF too large: {}", pdf_bytes.len());

        // Count /Type /Page minus /Type /Pages
        let page_count = pdf_bytes
            .windows(11)
            .filter(|w| w == b"/Type /Page")
            .count();
        assert!(page_count <= 3, "Too many pages: {page_count}");

        let _ = std::fs::remove_file(&tmp_html);
        let _ = std::fs::remove_file(&tmp_pdf);
    }
}
