//! ช่องรับคำสั่งจากภายนอกของโปรแกรมที่เปิดอยู่ (ตัวเชื่อม MCP ส่งคำสั่งมาทางนี้)
//!
//! เปิดพอร์ต TCP เฉพาะในเครื่อง (127.0.0.1) พอร์ตสุ่ม แล้วเขียนพอร์ต + รหัสลับไว้ที่ `info_path()`
//! ข้อความเป็น JSON บรรทัดเดียว: ขอ `{"token","tool","args"}` → ตอบ `{"ok":true,"result"}` หรือ `{"ok":false,"error"}`

use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use serde_json::{json, Value};

/// คำสั่งหนึ่งที่รอโปรแกรมทำ (ตอบกลับทาง `reply`)
pub struct Request {
    pub tool: String,
    pub args: Value,
    pub reply: Sender<Result<Value, String>>,
}

impl Request {
    pub fn answer(self, r: Result<Value, String>) {
        let _ = self.reply.send(r);
    }
}

/// ไฟล์บอกพอร์ตและรหัสลับของโปรแกรมที่เปิดอยู่
pub fn info_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("TadTor").join("control.json")
}

pub struct ControlServer {
    pub rx: Receiver<Request>,
    pub port: u16,
}

impl ControlServer {
    pub fn start(ctx: egui::Context) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let token = random_token();
        let path = info_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let info = json!({ "port": port, "token": token, "pid": std::process::id() });
        std::fs::write(&path, info.to_string()).map_err(|e| e.to_string())?;

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (tx, token, ctx) = (tx.clone(), token.clone(), ctx.clone());
                std::thread::spawn(move || serve(stream, &tx, &token, &ctx));
            }
        });
        Ok(Self { rx, port })
    }
}

/// ลบไฟล์บอกพอร์ตตอนปิดโปรแกรม (ถ้ายังเป็นของโปรแกรมนี้)
pub fn remove_info() {
    let path = info_path();
    let ours = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .is_some_and(|v| v["pid"].as_u64() == Some(std::process::id() as u64));
    if ours {
        let _ = std::fs::remove_file(path);
    }
}

fn random_token() -> String {
    let mut out = String::new();
    for _ in 0..2 {
        // RandomState สุ่มกุญแจใหม่ทุกครั้ง
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

fn serve(stream: TcpStream, tx: &Sender<Request>, token: &str, ctx: &egui::Context) {
    let Ok(read_half) = stream.try_clone() else { return };
    let mut line = String::new();
    if BufReader::new(read_half).read_line(&mut line).is_err() {
        return;
    }
    let result = match serde_json::from_str::<Value>(&line) {
        Err(e) => Err(format!("คำสั่งไม่ใช่ JSON: {e}")),
        Ok(msg) if msg["token"].as_str() != Some(token) => Err("รหัสลับไม่ถูกต้อง".to_owned()),
        Ok(msg) => {
            let (reply, wait) = mpsc::channel();
            let req = Request { tool: msg["tool"].as_str().unwrap_or_default().to_owned(), args: msg["args"].clone(), reply };
            if tx.send(req).is_err() {
                Err("โปรแกรมกำลังปิด".to_owned())
            } else {
                ctx.request_repaint();
                wait.recv().unwrap_or_else(|_| Err("โปรแกรมปิดไปก่อนทำเสร็จ".to_owned()))
            }
        }
    };
    let out = match result {
        Ok(v) => json!({ "ok": true, "result": v }),
        Err(e) => json!({ "ok": false, "error": e }),
    };
    let mut stream = stream;
    let _ = writeln!(stream, "{out}");
}
