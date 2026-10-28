//! ค้นหาและตรวจสอบ FFmpeg ในเครื่อง
//!
//! ตอนนี้โปรแกรมเรียกใช้ `ffmpeg.exe` เป็นโปรแกรมแยก (ติดตั้งง่ายกว่าลิงก์ไลบรารีเข้าไปตรง ๆ)
//! โค้ดส่วนที่คุยกับ FFmpeg จะอยู่ในโมดูลนี้ที่เดียว ถ้าวันหลังเปลี่ยนวิธีจะได้ไม่ต้องรื้อส่วนอื่น

pub mod graph;
pub mod probe;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub enum FfmpegStatus {
    Checking,
    NotFound,
    Found(FfmpegInfo),
}

#[derive(Clone, Debug)]
pub struct FfmpegInfo {
    pub ffmpeg: PathBuf,
    pub ffprobe: Option<PathBuf>,
    /// เช่น "7.1.1-full_build-www.gyan.dev"
    pub version: String,
    /// ทดลองเข้ารหัสด้วยการ์ด Nvidia (h264_nvenc) แล้วสำเร็จ
    pub nvenc_h264: bool,
    pub nvenc_hevc: bool,
}

/// ตัวตรวจ FFmpeg ที่ทำงานเบื้องหลัง (ไม่ให้หน้าจอค้างตอนเปิดโปรแกรม)
#[derive(Clone)]
pub struct FfmpegProbe {
    status: Arc<Mutex<FfmpegStatus>>,
}

impl FfmpegProbe {
    pub fn start(ctx: eframe::egui::Context) -> Self {
        let status = Arc::new(Mutex::new(FfmpegStatus::Checking));
        let probe = Self { status };
        probe.spawn(ctx);
        probe
    }

    /// ตรวจใหม่ (เช่น หลังผู้ใช้เพิ่งวางโฟลเดอร์ ffmpeg)
    pub fn recheck(&self, ctx: eframe::egui::Context) {
        *self.status.lock().unwrap() = FfmpegStatus::Checking;
        self.spawn(ctx);
    }

    pub fn status(&self) -> FfmpegStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn info(&self) -> Option<FfmpegInfo> {
        match self.status() {
            FfmpegStatus::Found(info) => Some(info),
            _ => None,
        }
    }

    /// รอจนตรวจเสร็จ (ใช้ในเธรดเบื้องหลังเท่านั้น ห้ามเรียกจากหน้าจอ)
    pub fn wait_info(&self) -> Option<FfmpegInfo> {
        loop {
            match self.status() {
                FfmpegStatus::Checking => std::thread::sleep(std::time::Duration::from_millis(50)),
                FfmpegStatus::NotFound => return None,
                FfmpegStatus::Found(info) => return Some(info),
            }
        }
    }

    fn spawn(&self, ctx: eframe::egui::Context) {
        let status = Arc::clone(&self.status);
        std::thread::spawn(move || {
            let result = detect();
            *status.lock().unwrap() = result;
            ctx.request_repaint();
        });
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// สร้างคำสั่งที่ไม่เด้งหน้าต่าง console ขึ้นมาบน Windows
pub fn command(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null());
    cmd
}

/// ที่ที่จะมองหา ffmpeg ตามลำดับ
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut push_base = |base: PathBuf| {
        dirs.push(base.join("ffmpeg").join("bin"));
        dirs.push(base.join("ffmpeg"));
        dirs.push(base);
    };
    // 1) โฟลเดอร์ ffmpeg ข้างไฟล์โปรแกรม (ตอนแจกจ่ายโปรแกรม)
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        push_base(dir);
    }
    // 2) โฟลเดอร์ ffmpeg ในโฟลเดอร์โปรเจกต์ (ตอนสั่ง cargo run)
    if let Ok(dir) = std::env::current_dir() {
        push_base(dir);
    }
    dirs
}

fn find_tool(name: &str) -> Option<PathBuf> {
    let file = exe(name);
    for dir in candidate_dirs() {
        let p = dir.join(&file);
        if p.is_file() {
            return Some(p);
        }
    }
    // 3) ที่ติดตั้งไว้ในระบบ (PATH) เช่นติดตั้งผ่าน winget
    let from_path = PathBuf::from(&file);
    let ok = command(&from_path).arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    ok.then_some(from_path)
}

fn detect() -> FfmpegStatus {
    let Some(ffmpeg) = find_tool("ffmpeg") else {
        return FfmpegStatus::NotFound;
    };
    let ffprobe = find_tool("ffprobe");

    let version = command(&ffmpeg)
        .arg("-version")
        .output()
        .ok()
        .and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout).into_owned();
            text.lines().next().map(|line| line.trim_start_matches("ffmpeg version ").split_whitespace().next().unwrap_or("").to_owned())
        })
        .unwrap_or_else(|| "ไม่ทราบรุ่น".to_owned());

    let nvenc_h264 = test_encoder(&ffmpeg, "h264_nvenc");
    let nvenc_hevc = nvenc_h264 && test_encoder(&ffmpeg, "hevc_nvenc");

    FfmpegStatus::Found(FfmpegInfo { ffmpeg, ffprobe, version, nvenc_h264, nvenc_hevc })
}

/// ลองเข้ารหัสวิดีโอสีดำสั้น ๆ ด้วย encoder นั้นจริง ๆ (ถ้าการ์ดจอ/ไดรเวอร์ไม่รองรับจะล้มเหลว)
pub fn test_encoder(ffmpeg: &Path, encoder: &str) -> bool {
    command(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=256x256:d=0.2",
            "-c:v",
            encoder,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
