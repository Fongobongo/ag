//! Test-only ACP agent: speaks minimal JSON-RPC 2.0 over stdio so the node
//! daemon's `drive_acp_session` can be exercised end-to-end without a real
//! agent. Not shipped; `CARGO_BIN_EXE_adapter-fake-acp` is referenced by the
//! node-daemon integration tests.

use serde_json::{json, Value};
use std::io::{BufRead, BufWriter, Write};

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let reader = std::io::BufReader::new(stdin.lock());
    let session_id = "sess-fake-1";

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = v.get("id").cloned().unwrap_or(Value::Null);
        match method {
            "initialize" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "protocol_version": "0.1", "capabilities": {}, "client": {} }
                });
                writeln!(out, "{}", resp).ok();
            }
            "session/new" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "session_id": session_id }
                });
                writeln!(out, "{}", resp).ok();
            }
            "session/prompt" => {
                // Test mode: hang mid-frame. Write the start of a JSON-RPC
                // `session/update` line with no terminating newline and block
                // forever, simulating an ACP subprocess that dies mid-frame
                // (truncated JSON). The node must time out and fail the attempt.
                if std::env::var_os("AG_FAKE_HANG").is_some() {
                    // Truncated JSON-RPC line (no close brace, no newline).
                    let partial = r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"type":"progress","text":"thi"#;
                    out.write_all(partial.as_bytes()).ok();
                    out.flush().ok();
                    // Block forever; the node's cancel/timeout must tear us down.
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(60));
                    }
                }
                // Emit two updates, then the prompt result.
                let u1 = json!({
                    "jsonrpc": "2.0",
                    "method": "session/update",
                    "params": { "update": { "type": "progress", "text": "thinking" } }
                });
                let u2 = json!({
                    "jsonrpc": "2.0",
                    "method": "session/update",
                    "params": { "update": { "type": "tool_call", "tool": "bash", "input": "echo hi" } }
                });
                writeln!(out, "{}", u1).ok();
                writeln!(out, "{}", u2).ok();
                out.flush().ok();
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "text": "done" }
                });
                writeln!(out, "{}", resp).ok();
            }
            "session/cancel" => {
                let resp = json!({ "jsonrpc": "2.0", "id": id, "result": {} });
                writeln!(out, "{}", resp).ok();
                out.flush().ok();
                break;
            }
            _ => {
                let resp = json!({ "jsonrpc": "2.0", "id": id, "result": {} });
                writeln!(out, "{}", resp).ok();
            }
        }
        out.flush().ok();
    }
}
