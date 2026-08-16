// TypePress HTTP rendering server (P2, v0.5.0).
//
// Minimal dependency choice: `tiny_http` (blocking, zero tokio/hyper family)
// keeps the single-binary, low-footprint positioning. One thread per request;
// rendering is CPU-bound so blocking threads are the natural fit.
//
// Endpoints:
//   GET  /healthz — {"status":"ok","version":"x.y.z"}
//   POST /render  — JSON body (html | markdown + options) → application/pdf
//
// Security posture:
//   - binds 127.0.0.1 by default (proxy through nginx/Caddy for exposure)
//   - request body size cap (--max-body, default 10 MiB)
//   - remote assets gated by AssetLimits (--max-asset-size / --allow-http /
//     --asset-allowlist) exactly like the CLI
//   - no file-system writes: input arrives as text, output is returned inline
use std::io::Read;
use std::sync::Arc;

use anyhow::Result;
use serde::Deserialize;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::cli::{Command, parse_margin};
use crate::render::{RenderOptions, render_document};
use typepress::network::AssetLimits;

/// JSON body of POST /render.
#[derive(Deserialize, Default)]
struct RenderRequest {
    /// Raw HTML document (mutually exclusive with `markdown`).
    html: Option<String>,
    /// Markdown source (mutually exclusive with `html`).
    markdown: Option<String>,
    /// Render options — mirrors the CLI flags.
    #[serde(default)]
    options: RequestOptions,
}

#[derive(Deserialize, Default)]
struct RequestOptions {
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    landscape: bool,
    /// Margin string accepted by parse_margin (e.g. "20mm", "10mm 15mm 10mm 15mm")
    #[serde(default)]
    margin: Option<String>,
    #[serde(default)]
    header: Option<String>,
    #[serde(default)]
    footer: Option<String>,
    #[serde(default)]
    math: bool,
    #[serde(default)]
    fit: bool,
    #[serde(default)]
    autofit: bool,
    #[serde(default = "default_zoom")]
    zoom: f32,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    no_system_fonts: bool,
    #[serde(default = "default_asset_size")]
    max_asset_size: u64,
    #[serde(default)]
    allow_http: bool,
    #[serde(default)]
    asset_allowlist: Vec<String>,
    /// Return HTTP 422 with warning JSON instead of a PDF when any warning fires.
    #[serde(default)]
    strict: bool,
}

fn default_zoom() -> f32 {
    1.0
}

fn default_asset_size() -> u64 {
    10 * 1024 * 1024
}

fn json_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
}

fn pdf_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"application/pdf"[..]).unwrap()
}

/// Start the server (called from main when `typepress serve` is invoked).
pub fn serve(cmd: Command) -> Result<()> {
    let Command::Serve {
        host,
        port,
        max_body,
    } = cmd;
    serve_impl(host, port, max_body)
}

fn serve_impl(host: String, port: u16, max_body: usize) -> Result<()> {
    let addr = format!("{host}:{port}");
    let server = Server::http(&addr).map_err(|e| anyhow::anyhow!("failed to bind {addr}: {e}"))?;
    let version = env!("CARGO_PKG_VERSION");

    eprintln!("TypePress {version} HTTP server listening on http://{addr}");
    eprintln!("  POST /render  — JSON {{html|markdown, options}} → application/pdf");
    eprintln!("  GET  /healthz — liveness probe");

    let shared = Arc::new(ServerState {
        max_body,
        version: version.to_string(),
    });

    for request in server.incoming_requests() {
        let shared = Arc::clone(&shared);
        // tiny_http blocks on incoming_requests; spawn so one slow render
        // doesn't stall the health probe or other requests.
        std::thread::spawn(move || {
            if let Err(e) = handle(&shared, request) {
                eprintln!("request error: {e:#}");
            }
        });
    }
    Ok(())
}

struct ServerState {
    max_body: usize,
    version: String,
}

fn handle(state: &ServerState, request: Request) -> Result<()> {
    let (method, url) = (request.method().clone(), request.url().to_string());

    // ── GET /healthz ──
    if method == Method::Get && url == "/healthz" {
        let body = format!(r#"{{"status":"ok","version":"{}"}}"#, state.version);
        request
            .respond(
                Response::from_string(body)
                    .with_status_code(StatusCode(200))
                    .with_header(json_header()),
            )
            .map_err(|e| anyhow::anyhow!("respond: {e}"))?;
        return Ok(());
    }

    // ── POST /render ──
    if method == Method::Post && url == "/render" {
        return handle_render(state, request);
    }

    // ── 404 ──
    request
        .respond(
            Response::from_string(format!("not found: {method} {url}"))
                .with_status_code(StatusCode(404)),
        )
        .map_err(|e| anyhow::anyhow!("respond: {e}"))?;
    Ok(())
}

fn handle_render(state: &ServerState, mut request: Request) -> Result<()> {
    // Body size cap (defense in depth — reader can't allocate beyond max_body+1)
    let mut body = Vec::new();
    let mut limited = request.as_reader().take((state.max_body as u64) + 1);
    limited.read_to_end(&mut body)?;
    if body.len() > state.max_body {
        request
            .respond(
                Response::from_string(format!(
                    r#"{{"error":"request body exceeds {} bytes"}}"#,
                    state.max_body
                ))
                .with_status_code(StatusCode(413))
                .with_header(json_header()),
            )
            .map_err(|e| anyhow::anyhow!("respond: {e}"))?;
        return Ok(());
    }

    // Parse JSON
    let req: RenderRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            request
                .respond(
                    Response::from_string(format!(
                        r#"{{"error":"invalid JSON: {}"}}"#,
                        e.to_string().replace('"', "\\\"")
                    ))
                    .with_status_code(StatusCode(400))
                    .with_header(json_header()),
                )
                .map_err(|e| anyhow::anyhow!("respond: {e}"))?;
            return Ok(());
        }
    };

    // Exactly one of html / markdown
    let (content, from) = match (&req.html, &req.markdown) {
        (Some(_), Some(_)) => {
            request
                .respond(
                    Response::from_string(
                        r#"{"error":"provide exactly one of 'html' or 'markdown'"}"#,
                    )
                    .with_status_code(StatusCode(400))
                    .with_header(json_header()),
                )
                .map_err(|e| anyhow::anyhow!("respond: {e}"))?;
            return Ok(());
        }
        (Some(h), None) => (h.clone(), "html".to_string()),
        (None, Some(m)) => (m.clone(), "md".to_string()),
        (None, None) => {
            request
                .respond(
                    Response::from_string(
                        r#"{"error":"provide 'html' or 'markdown' in the JSON body"}"#,
                    )
                    .with_status_code(StatusCode(400))
                    .with_header(json_header()),
                )
                .map_err(|e| anyhow::anyhow!("respond: {e}"))?;
            return Ok(());
        }
    };

    let o = &req.options;
    let opts = RenderOptions {
        content,
        from,
        size: o.size.clone(),
        landscape: o.landscape,
        margin: o.margin.as_deref().map(parse_margin),
        zoom: o.zoom,
        fit: o.fit,
        autofit: o.autofit,
        header: o.header.clone(),
        footer: o.footer.clone(),
        fonts: Vec::new(),
        css_files: Vec::new(),
        images: Vec::new(),
        math: o.math,
        math_dir: None,
        bookmarks: false,
        no_outline: false,
        tagged: false,
        pdf_ua: false,
        no_system_fonts: o.no_system_fonts,
        title: o.title.clone(),
        authors: o.authors.clone(),
        language: o.language.clone(),
        base_path: None,
        asset_limits: AssetLimits {
            max_bytes: o.max_asset_size,
            allow_http: o.allow_http,
            allowlist: o.asset_allowlist.clone(),
        },
        config: None,
    };

    match render_document(opts) {
        Ok(out) => {
            let warnings = out.diagnostics.warnings();
            let warn_count = warnings.len();
            if o.strict && warn_count > 0 {
                // strict: warnings are a failure — report them as JSON
                let codes: Vec<String> = warnings
                    .iter()
                    .map(|w| format!("[{}] {}", w.code, w.message))
                    .collect();
                request
                    .respond(
                        Response::from_string(
                            serde_json::json!({ "error": "strict mode: warnings", "warnings": codes })
                                .to_string(),
                        )
                        .with_status_code(StatusCode(422))
                        .with_header(json_header()),
                    )
                    .map_err(|e| anyhow::anyhow!("respond: {e}"))?;
                return Ok(());
            }
            // Success: PDF bytes
            for w in warnings {
                eprintln!("warning[{}]: {}", w.code, w.message);
            }
            let pages = out.pages.to_string();
            request
                .respond(
                    Response::from_data(out.pdf)
                        .with_status_code(StatusCode(200))
                        .with_header(pdf_header())
                        .with_header(
                            Header::from_bytes(&b"X-TypePress-Pages"[..], pages.as_bytes())
                                .unwrap(),
                        ),
                )
                .map_err(|e| anyhow::anyhow!("respond: {e}"))?;
            Ok(())
        }
        Err(e) => {
            eprintln!("render error: {e:#}");
            request
                .respond(
                    Response::from_string(
                        serde_json::json!({ "error": format!("{e:#}") }).to_string(),
                    )
                    .with_status_code(StatusCode(500))
                    .with_header(json_header()),
                )
                .map_err(|e| anyhow::anyhow!("respond: {e}"))?;
            Ok(())
        }
    }
}
