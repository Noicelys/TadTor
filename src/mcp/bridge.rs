//! `tadtor.exe --mcp`: เซิร์ฟเวอร์ MCP แบบ stdio ให้ AI (เช่น Claude) สั่งงานโปรแกรม
//!
//! รับ JSON-RPC จาก stdin ทีละบรรทัด ตอบทาง stdout แล้วส่งคำสั่งต่อไปยังโปรแกรมที่เปิดอยู่ผ่าน `control`
//! ถ้ายังไม่ได้เปิดโปรแกรม จะเปิดให้เองแล้วรอจนพร้อม

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::control::info_path;
use super::tools;

const PROTOCOL: &str = "2025-06-18";

const INSTRUCTIONS: &str = "TadTor is a desktop video editor (Thai UI) that is open on the user's screen. \
These tools edit its timeline live; every change can be undone by the user (Ctrl+Z) or with the `undo` tool. \
Times are in seconds on the timeline. Start with `get_timeline` to see clips and ids. \
To cut by speech: run `auto_subtitles` (or `get_subtitles` if subtitles exist), decide what to drop, then call `delete_ranges` \
with all ranges at once (later ranges are applied first, so pass times from the current timeline). \
`remove_silence` cuts pauses automatically. Main-track clips always play back to back; other tracks keep their times.";

pub fn run() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                send(&mut out, json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } }));
                continue;
            }
        };
        let Some(id) = msg.get("id").cloned() else { continue }; // การแจ้งเตือน ไม่ต้องตอบ
        let params = &msg["params"];
        let result = match msg["method"].as_str().unwrap_or_default() {
            "initialize" => Ok(json!({
                "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL),
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "tadtor", "title": "TadTor video editor", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::specs() })),
            "tools/call" => Ok(call(params["name"].as_str().unwrap_or_default(), params.get("arguments").cloned().unwrap_or(json!({})))),
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            m => Err(format!("ไม่รู้จักคำสั่ง {m}")),
        };
        let reply = match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": e } }),
        };
        send(&mut out, reply);
    }
}

fn send(out: &mut impl Write, v: Value) {
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn call(tool: &str, args: Value) -> Value {
    if !tools::specs().iter().any(|t| t["name"] == tool) {
        return json!({ "content": [{ "type": "text", "text": format!("ไม่มีเครื่องมือชื่อ {tool}") }], "isError": true });
    }
    match forward(tool, &args) {
        Ok(v) => {
            let text = if let Some(s) = v.as_str() { s.to_owned() } else { serde_json::to_string_pretty(&v).unwrap_or_default() };
            json!({ "content": [{ "type": "text", "text": text }], "isError": false })
        }
        Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
    }
}

/// พอร์ตและรหัสลับของโปรแกรมที่เปิดอยู่
fn read_info() -> Option<(u16, String)> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(info_path()).ok()?).ok()?;
    Some((v["port"].as_u64()? as u16, v["token"].as_str()?.to_owned()))
}

fn connect() -> Option<(TcpStream, String)> {
    let (port, token) = read_info()?;
    let s = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2)).ok()?;
    Some((s, token))
}

/// เปิดโปรแกรม TadTor แล้วรอจนรับคำสั่งได้
fn launch() -> Result<(TcpStream, String), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // ทำงานในโฟลเดอร์โปรแกรม (หา ffmpeg/whisper เจอ) ตอนรันจาก target\ ของ cargo ใช้โฟลเดอร์โปรเจกต์
    let dir = exe
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "target"))
        .and_then(|p| p.parent())
        .or_else(|| exe.parent())
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let _ = std::fs::remove_file(info_path());
    std::process::Command::new(&exe)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("เปิดโปรแกรม TadTor ไม่ได้: {e}"))?;
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(40) {
        if let Some(c) = connect() {
            return Ok(c);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err("เปิดโปรแกรม TadTor แล้วแต่รอนานเกินไป ลองเรียกใหม่อีกครั้ง".to_owned())
}

fn forward(tool: &str, args: &Value) -> Result<Value, String> {
    let (mut stream, token) = match connect() {
        Some(c) => c,
        None => launch()?,
    };
    writeln!(stream, "{}", json!({ "token": token, "tool": tool, "args": args })).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).map_err(|e| format!("ขาดการติดต่อกับโปรแกรม: {e}"))?;
    let v: Value = serde_json::from_str(&line).map_err(|_| "โปรแกรมปิดไประหว่างทำงาน".to_owned())?;
    if v["ok"].as_bool() == Some(true) {
        Ok(v["result"].clone())
    } else {
        Err(v["error"].as_str().unwrap_or("ทำไม่สำเร็จ").to_owned())
    }
}
