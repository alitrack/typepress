// Code syntax highlighting via syntect.
// Detects <pre><code class="language-xxx"> blocks and applies
// syntax-aware colorization using Sublime Text syntax definitions.
// Falls back to plain <pre><code> if language is unrecognized.

use anyhow::Result;
use regex::Regex;
use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;
use syntect::parsing::SyntaxSet;

/// Highlight code blocks in HTML. Uses syntect for syntax coloring.
pub fn highlight_code_blocks(html: &mut String) -> Result<usize> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    // Match <pre><code class="language-xxx">...</code></pre>
    let re = Regex::new(
        r#"<pre><code(?: class="language-(\w+)")?>([\s\S]*?)</code></pre>"#
    )?;

    let mut count = 0usize;
    let matches: Vec<_> = re.captures_iter(html)
        .map(|c| (c.get(0).unwrap().range(), c.get(1).map(|m| m.as_str().to_string()), c.get(2).unwrap().as_str().to_string()))
        .collect();

    for (range, lang, code) in matches.into_iter().rev() {
        let decoded = html_decode(&code);
        let syntax = lang.as_deref().and_then(|l| find_syntax(&ss, l));

        let highlighted = if let Some(syn) = syntax {
            match highlighted_html_for_string(&decoded, &ss, syn, theme) {
                Ok(h) => {
                    let inner = extract_body(&h);
                    format!(
                        "<pre style=\"background:#2b303b;color:#c0c5ce;padding:8pt;overflow-x:auto;font-size:8pt;line-height:1.4\"><code>{inner}</code></pre>"
                    )
                }
                Err(_) => format!("<pre><code>{}</code></pre>", html_escape(&decoded)),
            }
        } else {
            format!("<pre><code>{}</code></pre>", html_escape(&decoded))
        };

        html.replace_range(range, &highlighted);
        count += 1;
    }
    Ok(count)
}

fn extract_body(html: &str) -> String {
    if let Some(start) = html.find("<pre") {
        if let Some(body_start) = html[start..].find('>').map(|p| start + p + 1) {
            if let Some(end) = html.rfind("</pre>") {
                return html[body_start..end].to_string();
            }
        }
    }
    html.to_string()
}

fn find_syntax<'a>(ss: &'a SyntaxSet, lang: &str) -> Option<&'a syntect::parsing::SyntaxReference> {
    let name = match lang.to_lowercase().as_str() {
        "js" | "javascript" => "JavaScript",
        "ts" | "typescript" => "TypeScript",
        "py" | "python" => "Python",
        "rs" | "rust" => "Rust",
        "go" | "golang" => "Go",
        "c" => "C",
        "cpp" | "c++" => "C++",
        "java" => "Java",
        "sh" | "bash" | "shell" => "Bash",
        "sql" => "SQL",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "html" => "HTML",
        "css" => "CSS",
        "md" | "markdown" => "Markdown",
        "rb" | "ruby" => "Ruby",
        "php" => "PHP",
        "swift" => "Swift",
        "kt" | "kotlin" => "Kotlin",
        "scala" => "Scala",
        "r" => "R",
        "lua" => "Lua",
        "haskell" | "hs" => "Haskell",
        "dart" => "Dart",
        "elixir" | "ex" => "Elixir",
        "xml" => "XML",
        "dockerfile" | "docker" => "Dockerfile",
        "makefile" | "make" => "Makefile",
        "nginx" => "Nginx",
        "diff" | "patch" => "Diff",
        _ => return None,
    };
    ss.find_syntax_by_name(name)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn html_decode(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
        .replace("&quot;", "\"").replace("&#39;", "'")
}
