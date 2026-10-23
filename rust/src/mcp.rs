//! A synchronous, local-only MCP stdio server (2025-06-18 lifecycle and tools).
//! No HTTP server, background runtime, logging on stdout, or external processes.
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::{
    fs,
    io::{BufRead, Write},
    path::{Component, Path, PathBuf},
};

use crate::{assets, rendering};

const MAX_REQUEST: usize = 2_000_000;
const PROTOCOL_VERSION: &str = "2025-06-18";

/// Resolve the nearest existing ancestor as well as symlinks before an output is created.
pub fn resolved_path(path: &Path) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let mut current = path.as_path();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(current) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current.file_name().context("无效输出路径")?.to_owned());
                current = current.parent().context("无效输出路径")?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut resolved = current.canonicalize()?;
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

fn confined_path(root: &Path, requested: &str) -> Result<PathBuf> {
    ensure!(!requested.is_empty(), "输出路径不能为空");
    let requested = Path::new(requested);
    ensure!(
        !requested
            .components()
            .any(|part| matches!(part, Component::ParentDir)),
        "输出路径不能包含 .."
    );
    let path = if requested.is_absolute() {
        requested.to_owned()
    } else {
        root.join(requested)
    };
    let path = resolved_path(&path)?;
    ensure!(
        path != root && path.starts_with(root),
        "输出路径必须位于服务器 output-dir 内"
    );
    Ok(path)
}

fn tools() -> Value {
    json!({"tools": [
        {
            "name": "list_assets",
            "description": "列出完整内置字体、背景和允许写入的输出目录。builtin: 路径可以直接传给渲染工具。",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "render_handwriting",
            "description": "将纯文本生成 PNG/JPEG/WebP。字号 pt，位置/文字区域 px；font_spacing 为附加 em，line_spacing 为倍数。超出文字区域裁切，保持背景像素尺寸。仅向 output-dir 内写入，默认不覆盖。",
            "inputSchema": {
                "type": "object", "required": ["text", "output_path"], "additionalProperties": false,
                "properties": {
                    "text": {"type": "string", "maxLength": 100000},
                    "output_path": {"type": "string", "minLength": 1},
                    "preset_path": {"type": ["string", "null"]},
                    "overwrite": {"type": "boolean", "default": false},
                    "options": {
                        "type": ["object", "null"], "additionalProperties": false,
                        "properties": {
                            "font_path": {"type": "string"}, "bg_path": {"type": "string"},
                            "font_color": {"type": "string"},
                            "font_size": {"type": "integer", "minimum": 1, "maximum": 200},
                            "font_weight": {"type": "integer", "minimum": 100, "maximum": 900},
                            "x": {"type": "integer", "minimum": 0, "maximum": 8192},
                            "y": {"type": "integer", "minimum": 0, "maximum": 8192},
                            "rect_width": {"type": "integer", "minimum": 1, "maximum": 8192},
                            "rect_height": {"type": "integer", "minimum": 1, "maximum": 8192},
                            "font_spacing": {"type": "number", "minimum": -1, "maximum": 2},
                            "line_spacing": {"type": "number", "minimum": 0.1, "maximum": 5}
                        }
                    }
                }
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "openWorldHint": false}
        }
    ]})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCall {
    name: String,
    #[serde(default)]
    arguments: Map<String, Value>,
    #[serde(default, rename = "_meta")]
    _meta: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderArguments {
    text: String,
    output_path: String,
    preset_path: Option<PathBuf>,
    options: Option<Map<String, Value>>,
    #[serde(default)]
    overwrite: bool,
}

fn call_tool(call: ToolCall, root: &Path) -> Result<Value> {
    match call.name.as_str() {
        "list_assets" => {
            ensure!(call.arguments.is_empty(), "list_assets 不接受参数");
            let mut result = assets::list_assets();
            result["output_directory"] = json!(root);
            Ok(result)
        }
        "render_handwriting" => {
            let args: RenderArguments = serde_json::from_value(Value::Object(call.arguments))?;
            let path = confined_path(root, &args.output_path)?;
            let config = match args.preset_path {
                Some(path) => rendering::load_preset(&path)?,
                None => rendering::RenderConfig::default(),
            };
            let mut config = serde_json::to_value(config)?;
            let fields = config.as_object_mut().context("无效配置")?;
            if let Some(options) = args.options {
                ensure!(!options.contains_key("text"), "请通过 text 参数指定文字");
                fields.extend(options);
            }
            fields.insert("text".to_owned(), Value::String(args.text));
            let config: rendering::RenderConfig = serde_json::from_value(config)?;
            let image = rendering::render_image(&config)?;
            // Check again immediately before saving, including any newly created parent symlinks.
            let path = confined_path(root, path.to_str().context("无效输出路径编码")?)?;
            let path = rendering::save_image(&image, &path, args.overwrite)?;
            Ok(json!({"path": path, "width": image.width(), "height": image.height()}))
        }
        _ => bail!("未知工具：{}", call.name),
    }
}

fn rpc_error(id: Value, code: i32, message: impl ToString) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.to_string()}})
}

#[derive(Default)]
struct Session {
    initialized: bool,
    ready: bool,
}

fn dispatch(message: Value, session: &mut Session, root: &Path) -> Option<Value> {
    let Some(object) = message.as_object() else {
        return Some(rpc_error(
            Value::Null,
            -32600,
            "Expected one JSON-RPC object",
        ));
    };
    if object.get("jsonrpc") != Some(&json!("2.0")) {
        return Some(rpc_error(Value::Null, -32600, "Expected jsonrpc 2.0"));
    }
    // No server-initiated requests are used; ignore responses to avoid response loops.
    if !object.contains_key("method")
        && (object.contains_key("result") || object.contains_key("error"))
    {
        return None;
    }
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return Some(rpc_error(Value::Null, -32600, "Missing method"));
    };
    let Some(id) = object.get("id").cloned() else {
        if method == "notifications/initialized" && session.initialized {
            session.ready = true;
        }
        return None;
    };
    if !id.is_string() && !id.is_i64() && !id.is_u64() {
        return Some(rpc_error(
            Value::Null,
            -32600,
            "Request id must be a string or integer",
        ));
    }
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Some(rpc_error(id, -32602, "params must be an object"));
    }
    let result = match method {
        "initialize" => {
            if session.initialized {
                return Some(rpc_error(id, -32600, "Already initialized"));
            }
            let version = params.get("protocolVersion").and_then(Value::as_str);
            if version.is_none()
                || !params["capabilities"].is_object()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
            {
                return Some(rpc_error(
                    id,
                    -32602,
                    "Expected protocolVersion, capabilities and clientInfo",
                ));
            }
            let version = match version.unwrap() {
                "2024-11-05" => "2024-11-05",
                "2025-03-26" => "2025-03-26",
                _ => PROTOCOL_VERSION,
            };
            session.initialized = true;
            json!({"protocolVersion": version, "capabilities": {"tools": {}},
                "serverInfo": {"name": "xingjian", "title": "星笺", "version": env!("CARGO_PKG_VERSION")}})
        }
        "ping" => json!({}),
        _ if !session.ready => {
            return Some(rpc_error(id, -32002, "Initialize the MCP session first"));
        }
        "tools/list" => {
            if params.get("cursor").is_some() {
                return Some(rpc_error(id, -32602, "No pagination cursor is available"));
            }
            tools()
        }
        "tools/call" => {
            let call: ToolCall = match serde_json::from_value(params) {
                Ok(call) => call,
                Err(error) => return Some(rpc_error(id, -32602, error)),
            };
            if !matches!(call.name.as_str(), "list_assets" | "render_handwriting") {
                return Some(rpc_error(id, -32602, "Unknown tool"));
            }
            match call_tool(call, root) {
                Ok(result) => {
                    json!({"content": [{"type": "text", "text": result.to_string()}], "isError": false})
                }
                Err(error) => {
                    json!({"content": [{"type": "text", "text": format!("{error:#}")}], "isError": true})
                }
            }
        }
        _ => return Some(rpc_error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// Read one newline-delimited frame with bounded memory, draining an oversized frame.
fn read_frame(input: &mut impl BufRead) -> std::io::Result<Option<(Vec<u8>, bool)>> {
    let mut frame = Vec::new();
    let mut too_large = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(if frame.is_empty() && !too_large {
                None
            } else {
                Some((frame, too_large))
            });
        }
        let end = available.iter().position(|&byte| byte == b'\n');
        let count = end.map_or(available.len(), |index| index + 1);
        if frame.len() + count > MAX_REQUEST {
            too_large = true;
        }
        if !too_large {
            frame.extend_from_slice(&available[..count]);
        }
        input.consume(count);
        if end.is_some() {
            return Ok(Some((frame, too_large)));
        }
    }
}

pub fn run(output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    let root = output_dir.canonicalize()?;
    let mut input = std::io::stdin().lock();
    let mut output = std::io::BufWriter::new(std::io::stdout().lock());
    let mut session = Session::default();
    while let Some((frame, too_large)) = read_frame(&mut input)? {
        let response = if too_large {
            Some(rpc_error(
                Value::Null,
                -32600,
                "Request exceeds 2000000 bytes",
            ))
        } else {
            match serde_json::from_slice::<Value>(&frame) {
                Ok(message) => dispatch(message, &mut session, &root),
                Err(error) => Some(rpc_error(Value::Null, -32700, error)),
            }
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}
