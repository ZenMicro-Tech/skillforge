//! Minimal MCP stdio server: newline-delimited JSON-RPC 2.0, one tool.
//!
//! Handles: `initialize`, `notifications/initialized`, `tools/list`, `tools/call`.
//! Anything else returns method-not-found. This is deliberately minimal — Phase 1
//! only needs enough surface to let Claude Code call a skill as a tool.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
];

#[derive(Clone)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

pub fn serve<F>(tools: Vec<ToolDescriptor>, mut handler: F) -> Result<()>
where
    F: FnMut(&str, Value) -> Result<Value>,
{
    serve_dynamic(move || tools.clone(), &mut handler)
}

/// Variant where the tool list is re-resolved on each request — used by mux mode
/// so newly-added skills appear without restarting the server.
pub fn serve_dynamic<L, F>(mut list_tools: L, handler: &mut F) -> Result<()>
where
    L: FnMut() -> Vec<ToolDescriptor>,
    F: FnMut(&str, Value) -> Result<Value>,
{
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if req.id.is_none() {
            continue;
        }
        let tools = list_tools();
        let response = handle(&req, &tools, handler);
        let payload = serde_json::to_string(&response)?;
        writeln!(out, "{payload}")?;
        out.flush()?;
    }
    Ok(())
}

fn handle<F>(req: &Request, tools: &[ToolDescriptor], handler: &mut F) -> Response
where
    F: FnMut(&str, Value) -> Result<Value>,
{
    match req.method.as_str() {
        "initialize" => {
            let client_version = req
                .params
                .as_ref()
                .and_then(|p| p.get("protocolVersion"))
                .and_then(Value::as_str);
            let negotiated = match client_version {
                Some(v) if SUPPORTED_PROTOCOL_VERSIONS.contains(&v) => v,
                _ => LATEST_PROTOCOL_VERSION,
            };
            Response::ok(
                req.id.clone(),
                json!({
                    "protocolVersion": negotiated,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "skillforge", "version": env!("CARGO_PKG_VERSION") },
                }),
            )
        }
        "tools/list" => {
            let listed: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                    })
                })
                .collect();
            Response::ok(req.id.clone(), json!({ "tools": listed }))
        }
        "tools/call" => call_tool(req, tools, handler),
        "ping" => Response::ok(req.id.clone(), json!({})),
        _ => Response::error(req.id.clone(), -32601, "method not found"),
    }
}

fn call_tool<F>(req: &Request, tools: &[ToolDescriptor], handler: &mut F) -> Response
where
    F: FnMut(&str, Value) -> Result<Value>,
{
    let params = req.params.clone().unwrap_or(Value::Null);
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    if !tools.iter().any(|t| t.name == name) {
        return Response::error(req.id.clone(), -32602, "unknown tool");
    }
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    match handler(name, args) {
        Ok(v) => Response::ok(
            req.id.clone(),
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string(&v).unwrap_or_default(),
                }],
                "structuredContent": v,
                "isError": false,
            }),
        ),
        Err(e) => Response::ok(
            req.id.clone(),
            json!({
                "content": [{ "type": "text", "text": e.to_string() }],
                "isError": true,
            }),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct Request {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: &'static str,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ResponseError>,
}

#[derive(Debug, Serialize)]
struct ResponseError {
    code: i32,
    message: String,
}

impl Response {
    fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }
    fn error(id: Option<Value>, code: i32, message: &str) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(ResponseError {
                code,
                message: message.to_string(),
            }),
        }
    }
}
