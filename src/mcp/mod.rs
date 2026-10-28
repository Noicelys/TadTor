//! เชื่อมต่อ AI ผ่าน MCP (Model Context Protocol)
//!
//! - `control`: ช่องรับคำสั่งในโปรแกรมที่เปิดอยู่ (TCP เฉพาะในเครื่อง)
//! - `bridge`: `tadtor.exe --mcp` เซิร์ฟเวอร์ MCP แบบ stdio ที่ AI เรียก แล้วส่งต่อมาที่ `control`
//! - `tools`: รายชื่อเครื่องมือและการทำงานจริง
//!
//! ตั้งค่าใน Claude Code: `claude mcp add tadtor -- "<ที่อยู่ tadtor.exe>" --mcp`

pub mod bridge;
pub mod control;
pub mod tools;

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::app::TadTorApp;
use control::{ControlServer, Request};
use tools::Outcome;

/// คำสั่งที่ยังทำไม่เสร็จ (รอไฟล์อ่านเสร็จ / รอถอดเสียง / รอหาช่วงเงียบ)
pub enum Pending {
    /// ลองรันคำสั่งเดิมใหม่รอบหน้า
    Retry,
    Subtitles,
    Silence {
        rx: Receiver<Result<Vec<(f64, f64)>, String>>,
        remove: bool,
        pad: f64,
    },
}

#[derive(Default)]
pub struct McpState {
    server: Option<ControlServer>,
    pub error: Option<String>,
    waiting: Vec<(Request, Pending, Instant)>,
}

impl McpState {
    pub fn start(ctx: egui::Context) -> Self {
        match ControlServer::start(ctx) {
            Ok(server) => Self { server: Some(server), ..Self::default() },
            Err(e) => Self { error: Some(e), ..Self::default() },
        }
    }

    pub fn port(&self) -> Option<u16> {
        self.server.as_ref().map(|s| s.port)
    }

    /// มีคำสั่งจาก AI ค้างอยู่หรือไม่ (ไว้แสดงสถานะ)
    pub fn busy(&self) -> bool {
        !self.waiting.is_empty()
    }
}

/// คำสั่ง AI สำหรับตั้งค่าใน Claude Code
pub fn claude_command() -> String {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "tadtor.exe".to_owned());
    format!("claude mcp add tadtor -- \"{exe}\" --mcp")
}

/// ทำคำสั่งที่เข้ามาใหม่และคำสั่งที่รออยู่ (เรียกทุกรอบวาด)
pub fn poll(app: &mut TadTorApp, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.mcp);
    let mut waiting = std::mem::take(&mut state.waiting);
    if let Some(server) = &state.server {
        while let Ok(req) = server.rx.try_recv() {
            waiting.push((req, Pending::Retry, Instant::now()));
        }
    }
    let mut still = Vec::new();
    for (req, pending, since) in waiting {
        let outcome = match pending {
            Pending::Retry if since.elapsed() > Duration::from_secs(60) => Outcome::Done(Err("รอนานเกินไป (อ่านไฟล์ไม่เสร็จ)".to_owned())),
            Pending::Retry => tools::run(app, ctx, &req),
            Pending::Subtitles => match tools::subtitles_result(app) {
                Some(r) => Outcome::Done(r),
                None => Outcome::Wait(Pending::Subtitles),
            },
            Pending::Silence { rx, remove, pad } => match rx.try_recv() {
                Ok(Ok(found)) => Outcome::Done(tools::silence_result(app, ctx, found, remove, pad)),
                Ok(Err(e)) => Outcome::Done(Err(e)),
                Err(std::sync::mpsc::TryRecvError::Empty) => Outcome::Wait(Pending::Silence { rx, remove, pad }),
                Err(_) => Outcome::Done(Err("หาช่วงเงียบไม่สำเร็จ".to_owned())),
            },
        };
        match outcome {
            Outcome::Done(r) => req.answer(r),
            Outcome::Wait(p) => still.push((req, p, since)),
        }
    }
    if !still.is_empty() {
        ctx.request_repaint_after(Duration::from_millis(150));
    }
    state.waiting = still;
    app.mcp = state;
}
