//! ternlang-lsp — Language Server Protocol implementation for ternlang
//!
//! Implements LSP 3.17 over JSON-RPC 2.0 (stdio transport).
//! Provides: hover, diagnostics, completion, go-to-definition for .tern files.
//!
//! Usage: configure your editor to run `ternlang-lsp` as the language server
//! for `.tern` files. See the VS Code extension for a reference client setup.

use std::io::{BufRead, BufReader, Write};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ternlang_core::parser::{Parser, ParseError};

// ─────────────────────────────────────────────────────────────────────────────
// LSP JSON-RPC types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RpcMessage {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: Option<String>,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

impl RpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        RpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None }
    }
    fn err(id: Value, code: i32, message: &str) -> Self {
        RpcResponse {
            jsonrpc: "2.0", id,
            result: None,
            error: Some(json!({ "code": code, "message": message })),
        }
    }
}

fn send_response(resp: &RpcResponse) {
    let body = serde_json::to_string(resp).unwrap();
    print!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    std::io::stdout().flush().unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Diagnostic extraction from parse errors
// ─────────────────────────────────────────────────────────────────────────────

fn parse_errors_to_diagnostics(source: &str) -> Vec<Value> {
    let mut parser = Parser::new(source);
    let mut diagnostics = Vec::new();

    // Try full program parse first
    match parser.parse_program() {
        Ok(_) => {} // clean
        Err(e) => {
            let message = format_parse_error(&e);
            // Estimate line: scan for error token position (simplified)
            let line = estimate_error_line(source, &e);
            diagnostics.push(json!({
                "range": {
                    "start": { "line": line, "character": 0 },
                    "end":   { "line": line, "character": 100 }
                },
                "severity": 1,  // Error
                "source": "ternlang",
                "message": message
            }));
        }
    }
    diagnostics
}

fn format_parse_error(e: &ParseError) -> String {
    match e {
        ParseError::UnexpectedToken(t)    => format!("Unexpected token: {}", t),
        ParseError::ExpectedToken(exp, got) => format!("Expected {} but got {}", exp, got),
        ParseError::InvalidTrit(t)        => format!("Invalid trit literal: {}", t),
        ParseError::NonExhaustiveMatch(m) => format!("Non-exhaustive match: {}", m),
    }
}

fn estimate_error_line(source: &str, _e: &ParseError) -> u32 {
    // Simplified: return the line count as a rough estimate.
    // A real LSP would track character offsets through the lexer.
    (source.lines().count().saturating_sub(1)) as u32
}

// ─────────────────────────────────────────────────────────────────────────────
// Hover documentation
// ─────────────────────────────────────────────────────────────────────────────

fn hover_for_word(word: &str) -> Option<&'static str> {
    match word {
        "trit"       => Some("**trit** — balanced ternary value: `-1` (conflict), `0` (hold — active, not null), `+1` (truth)"),
        "trittensor" => Some("**trittensor<N x M>** — N×M balanced ternary tensor. Use `@sparseskip` for zero-weight skipping."),
        "agentref"   => Some("**agentref** — handle to a running ternlang agent instance"),
        "consensus"  => Some("**consensus(a, b) → trit** — ternary OR: agrees with both operands when equal, else hold"),
        "invert"     => Some("**invert(x) → trit** — negate trit: +1↔-1, 0→0"),
        "matmul"     => Some("**matmul(A, B) → trittensor** — dense matrix multiply. Add `@sparseskip` to skip zero weights."),
        "sparseskip" => Some("**@sparseskip** — directive: routes `matmul()` to `TSPARSE_MATMUL`, skipping zero-weight elements at the VM level"),
        "match"      => Some("**match** — 3-way exhaustive pattern match. All three arms (-1, 0, +1) are **required** or the compiler rejects it."),
        "agent"      => Some("**agent Name { fn handle(msg: trit) -> trit { ... } }** — define an actor with a message handler"),
        "spawn"      => Some("**spawn AgentName** → agentref — create a local agent instance\n**spawn remote \"addr\" AgentName** → agentref — spawn on a remote node"),
        "send"       => Some("**send agentref message** — enqueue a trit message in the agent's mailbox (non-blocking)"),
        "await"      => Some("**await agentref** → trit — dequeue from agent mailbox, run handler, return result"),
        "truth"      => Some("**truth() → trit** — returns +1"),
        "hold"       => Some("**hold() → trit** — returns 0 (active neutral state, NOT null)"),
        "conflict"   => Some("**conflict() → trit** — returns -1"),
        "cast"       => Some("**cast(expr) → trit** — type coercion to trit (transparent at VM level)"),
        "for"        => Some("**for x in tensor { }** — iterate over trit elements of a tensor"),
        "loop"       => Some("**loop { }** — infinite loop, exit with `break`"),
        "struct"     => Some("**struct Name { field: trit, ... }** — define a struct with trit/tensor fields"),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Completion items
// ─────────────────────────────────────────────────────────────────────────────

fn get_completions() -> Value {
    let keywords = vec![
        ("fn",          "function",    "fn ${1:name}(${2:param}: trit) -> trit {\n\t$0\n}"),
        ("let",         "variable",    "let ${1:name}: trit = $0;"),
        ("match",       "match",       "match ${1:expr} {\n\t 1 => { $2 }\n\t 0 => { $3 }\n\t-1 => { $4 }\n}"),
        ("if",          "if-ternary",  "if ${1:cond} ? {\n\t$2\n} else {\n\t$3\n} else {\n\t$4\n}"),
        ("for",         "for-in",      "for ${1:item} in ${2:tensor} {\n\t$0\n}"),
        ("loop",        "loop",        "loop {\n\t$0\n}"),
        ("agent",       "agent",       "agent ${1:Name} {\n\tfn handle(msg: trit) -> trit {\n\t\t$0\n\t}\n}"),
        ("struct",      "struct",      "struct ${1:Name} {\n\t${2:field}: trit,\n}"),
        ("@sparseskip", "directive",   "@sparseskip let ${1:out}: trittensor<${2:1 x 1}> = matmul($3, $4);"),
        ("consensus",   "builtin",     "consensus(${1:a}, ${2:b})"),
        ("invert",      "builtin",     "invert(${1:x})"),
        ("matmul",      "builtin",     "matmul(${1:A}, ${2:B})"),
        ("sparsity",    "builtin",     "sparsity(${1:t})"),
        ("spawn",       "actor",       "spawn ${1:AgentName}"),
        ("send",        "actor",       "send ${1:agent} ${2:signal};"),
        ("await",       "actor",       "await ${1:agent}"),
        ("truth",       "builtin",     "truth()"),
        ("hold",        "builtin",     "hold()"),
        ("conflict",    "builtin",     "conflict()"),
    ];

    let items: Vec<Value> = keywords.iter().map(|(label, detail, insert)| {
        json!({
            "label": label,
            "detail": detail,
            "insertText": insert,
            "insertTextFormat": 2,  // snippet
            "kind": 14  // keyword
        })
    }).collect();

    json!({ "isIncomplete": false, "items": items })
}

// ─────────────────────────────────────────────────────────────────────────────
// Document store (simplified — one document per session)
// ─────────────────────────────────────────────────────────────────────────────

struct DocumentStore {
    content: std::collections::HashMap<String, String>,
}

impl DocumentStore {
    fn new() -> Self { DocumentStore { content: std::collections::HashMap::new() } }
    fn update(&mut self, uri: &str, text: &str) { self.content.insert(uri.to_string(), text.to_string()); }
    fn get(&self, uri: &str) -> Option<&String> { self.content.get(uri) }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main LSP loop
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut store = DocumentStore::new();

    loop {
        // Read Content-Length header
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 { break; }
        let header = header.trim();
        if !header.starts_with("Content-Length:") { continue; }
        let length: usize = header["Content-Length:".len()..].trim().parse().unwrap_or(0);
        if length == 0 { continue; }

        // Consume blank line
        let mut blank = String::new();
        reader.read_line(&mut blank).unwrap_or(0);

        // Read body
        let mut body = vec![0u8; length];
        use std::io::Read;
        if reader.read_exact(&mut body).is_err() { break; }
        let body = String::from_utf8_lossy(&body);

        let msg: RpcMessage = match serde_json::from_str(&body) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let id = msg.id.clone().unwrap_or(Value::Null);
        let method = msg.method.as_deref().unwrap_or("");

        match method {
            "initialize" => {
                send_response(&RpcResponse::ok(id, json!({
                    "capabilities": {
                        "textDocumentSync": 1,  // full sync
                        "hoverProvider": true,
                        "completionProvider": { "triggerCharacters": [".", "@"] },
                        "diagnosticProvider": {
                            "interFileDependencies": false,
                            "workspaceDiagnostics": false
                        }
                    },
                    "serverInfo": { "name": "ternlang-lsp", "version": "0.1.0" }
                })));
            }

            "initialized" | "$/cancelRequest" => {
                // No response needed for notifications
            }

            "textDocument/didOpen" => {
                if let Some(params) = &msg.params {
                    let uri  = params["textDocument"]["uri"].as_str().unwrap_or("").to_string();
                    let text = params["textDocument"]["text"].as_str().unwrap_or("").to_string();
                    store.update(&uri, &text);
                    publish_diagnostics(&uri, &text);
                }
            }

            "textDocument/didChange" => {
                if let Some(params) = &msg.params {
                    let uri = params["textDocument"]["uri"].as_str().unwrap_or("").to_string();
                    if let Some(text) = params["contentChanges"][0]["text"].as_str() {
                        store.update(&uri, text);
                        publish_diagnostics(&uri, text);
                    }
                }
            }

            "textDocument/hover" => {
                let word = extract_hover_word(&msg.params, &store);
                let result = if let Some(w) = word.as_deref().and_then(hover_for_word) {
                    json!({ "contents": { "kind": "markdown", "value": w } })
                } else {
                    Value::Null
                };
                send_response(&RpcResponse::ok(id, result));
            }

            "textDocument/completion" => {
                send_response(&RpcResponse::ok(id, get_completions()));
            }

            "shutdown" => {
                send_response(&RpcResponse::ok(id, Value::Null));
            }

            "exit" => break,

            _ => {
                // Unknown method — respond with method not found
                if msg.id.is_some() {
                    send_response(&RpcResponse::err(id, -32601, "Method not found"));
                }
            }
        }
    }
}

fn publish_diagnostics(uri: &str, text: &str) {
    let diagnostics = parse_errors_to_diagnostics(text);
    let notif = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": diagnostics
        }
    });
    let body = serde_json::to_string(&notif).unwrap();
    print!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    std::io::stdout().flush().unwrap();
}

fn extract_hover_word(params: &Option<Value>, store: &DocumentStore) -> Option<String> {
    let params = params.as_ref()?;
    let uri  = params["textDocument"]["uri"].as_str()?;
    let line = params["position"]["line"].as_u64()? as usize;
    let char = params["position"]["character"].as_u64()? as usize;
    let text = store.get(uri)?;
    let source_line = text.lines().nth(line)?;
    // Extract the word at `char` position
    let bytes = source_line.as_bytes();
    let mut start = char;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    let mut end = char;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    Some(source_line[start..end].to_string())
}
