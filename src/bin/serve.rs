//! Minimal HTTP server — serves the D3.js ontology visualizer.
//!
//! Run:  cargo run --bin serve
//!       then open http://localhost:3000
//!
//! Requires ontology_graph.json to exist (run any example first that exports it,
//! e.g.  cargo run --example 02_ontology  or  cargo run --example 05_complex_discovery).
//!
//! Zero external dependencies — uses only std::net.
//!
//! ─── Dioxus integration note ─────────────────────────────────────────────────
//! If you prefer a full Dioxus web app instead, the pattern is:
//!
//!   // Cargo.toml: dioxus = { version = "0.6", features = ["web"] }
//!
//!   use dioxus::prelude::*;
//!   const GRAPH_JSON: &str = include_str!("../../ontology_graph.json");
//!
//!   fn Graph() -> Element {
//!       let loaded = use_signal(|| false);
//!       use_effect(move || {
//!           if loaded() {
//!               let js = format!("window.renderOntologyGraph({})", GRAPH_JSON);
//!               document::eval(&js);
//!           }
//!       });
//!       rsx! { div { id: "ontology-graph", onmounted: move |_| loaded.set(true) } }
//!   }
//!
//!   fn main() { dioxus::launch(Graph); }
//!
//! The index.html below (under assets/) works identically for both approaches.
//! ─────────────────────────────────────────────────────────────────────────────

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

fn main() {
    let addr = "127.0.0.1:3000";
    let listener = TcpListener::bind(addr).expect("Cannot bind to port 3000");

    println!("╔═══════════════════════════════════════════════════════╗");
    println!("║   Palantir Ontology Visualizer                       ║");
    println!("╠═══════════════════════════════════════════════════════╣");
    println!("║   http://{}                      ║", addr);
    println!("║                                                       ║");
    println!("║   Requires: ontology_graph.json (run example 02 or   ║");
    println!("║             05 first to generate it)                  ║");
    println!("║                                                       ║");
    println!("║   Press Ctrl-C to stop                                ║");
    println!("╚═══════════════════════════════════════════════════════╝");

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };

        let reader   = BufReader::new(&stream);
        let req_line = reader.lines().next()
            .and_then(|l| l.ok())
            .unwrap_or_default();

        // Parse: "GET /path HTTP/1.1"
        let path = req_line.split_whitespace().nth(1).unwrap_or("/").to_string();

        let (status, content_type, body) = match path.as_str() {
            "/" | "/index.html" => {
                let html = fs::read_to_string("assets/index.html")
                    .unwrap_or_else(|_| "<h1>assets/index.html not found</h1>".to_string());
                ("200 OK", "text/html; charset=utf-8", html)
            }
            "/ontology_graph.json" => {
                match fs::read_to_string("ontology_graph.json") {
                    Ok(json) => ("200 OK", "application/json", json),
                    Err(_) => (
                        "404 Not Found",
                        "application/json",
                        r#"{"error":"ontology_graph.json not found. Run: cargo run --example 02_ontology"}"#.into(),
                    ),
                }
            }
            _ => ("404 Not Found", "text/plain", "Not Found".into()),
        };

        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
            status, content_type, body.len(), body
        );

        let _ = stream.write_all(response.as_bytes());
        println!("  {} → {}", path, status);
    }
}
