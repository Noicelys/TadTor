//! ส่งออกไฟล์ MP4 ในเธรดเบื้องหลัง พร้อมบอกความคืบหน้า

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;

use crate::core::timeline::Timeline;
use crate::ffmpeg::graph::{self, VideoSpec};
use crate::ffmpeg::{command, probe::error_line};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoder {
    /// การ์ดจอ Nvidia (เร็วมาก)
    Nvenc,
    /// CPU (libx264) ช้ากว่าแต่ใช้ได้ทุกเครื่อง
    Cpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264,
    /// ไฟล์เล็กกว่า H.264 ราวครึ่งที่ความชัดเท่ากัน แต่เครื่องเก่าบางเครื่องเล่นไม่ได้
    H265,
}

impl Codec {
    pub const ALL: [Codec; 2] = [Codec::H264, Codec::H265];

    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264 (เล่นได้ทุกที่)",
            Codec::H265 => "H.265 (ไฟล์เล็กกว่า)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    High,
    Standard,
    Small,
    /// ตั้งค่า CRF/CQ เอง
    Custom,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::High, Quality::Standard, Quality::Small, Quality::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Quality::High => "สูง",
            Quality::Standard => "มาตรฐาน",
            Quality::Small => "ไฟล์เล็ก",
            Quality::Custom => "กำหนดเอง",
        }
    }

    /// ค่า CRF (CPU) / CQ (NVENC) ยิ่งน้อยยิ่งชัด ไฟล์ยิ่งใหญ่
    pub fn crf(self, encoder: Encoder, custom: u32) -> u32 {
        match (self, encoder) {
            (Quality::Custom, _) => custom,
            (Quality::High, Encoder::Nvenc) => 19,
            (Quality::Standard, Encoder::Nvenc) => 23,
            (Quality::Small, Encoder::Nvenc) => 28,
            (Quality::High, Encoder::Cpu) => 18,
            (Quality::Standard, Encoder::Cpu) => 21,
            (Quality::Small, Encoder::Cpu) => 26,
        }
    }
}

/// ความเร็วในการเข้ารหัส (ช้ากว่า = ไฟล์เล็กกว่าที่ความชัดเท่ากัน)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    Fast,
    Balanced,
    Best,
}

impl Speed {
    pub const ALL: [Speed; 3] = [Speed::Fast, Speed::Balanced, Speed::Best];

    pub fn label(self) -> &'static str {
        match self {
            Speed::Fast => "เร็ว",
            Speed::Balanced => "สมดุล",
            Speed::Best => "ละเอียด",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportSettings {
    pub path: PathBuf,
    pub video: VideoSpec,
    pub encoder: Encoder,
    pub codec: Codec,
    pub quality: Quality,
    /// ใช้เมื่อ `quality` = กำหนดเอง
    pub crf: u32,
    pub speed: Speed,
    /// kbps
    pub audio_bitrate: u32,
}

#[derive(Clone, Debug)]
pub enum ExportState {
    Running { progress: f32, started: Instant },
    Done { path: PathBuf, seconds: f64 },
    Failed(String),
    Cancelled,
}

pub struct ExportJob {
    pub state: Arc<Mutex<ExportState>>,
    child: Arc<Mutex<Option<Child>>>,
}

impl ExportJob {
    pub fn start(ffmpeg: &Path, tl: &Timeline, settings: ExportSettings, ctx: egui::Context) -> Self {
        let started = Instant::now();
        let state = Arc::new(Mutex::new(ExportState::Running { progress: 0.0, started }));
        let child: Arc<Mutex<Option<Child>>> = Arc::default();
        let job = Self { state: Arc::clone(&state), child: Arc::clone(&child) };

        let total = tl.duration();
        let g = graph::build(tl, 0.0, total, Some(settings.video), Some(48_000));
        // graph ยาวได้มากเมื่อมีหลายคลิป เก็บลงไฟล์แทนการใส่ในบรรทัดคำสั่ง (Windows จำกัดความยาว)
        let graph_file = std::env::temp_dir().join(format!("tadtor-export-{}.txt", std::process::id()));
        if let Err(e) = std::fs::write(&graph_file, &g.filter) {
            *state.lock().unwrap() = ExportState::Failed(format!("เขียนไฟล์ชั่วคราวไม่ได้: {e}"));
            return job;
        }

        let mut cmd = command(ffmpeg);
        cmd.current_dir(&g.cwd)
            .args(["-y", "-hide_banner", "-v", "error", "-nostdin", "-progress", "pipe:1", "-nostats"])
            .args(&g.inputs)
            .arg("-/filter_complex")
            .arg(&graph_file)
            .args(["-map", "[vout]", "-map", "[aout]"])
            .args(encoder_args(&settings))
            .args(["-r", &settings.video.fps.to_string()])
            .args(["-c:a", "aac", "-b:a", &format!("{}k", settings.audio_bitrate), "-ar", "48000", "-movflags", "+faststart"])
            .args(["-t", &format!("{total:.6}")])
            .arg(&settings.path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut spawned = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                *state.lock().unwrap() = ExportState::Failed(format!("เรียก FFmpeg ไม่ได้: {e}"));
                return job;
            }
        };
        let stdout = spawned.stdout.take().unwrap();
        let mut stderr = spawned.stderr.take().unwrap();
        *child.lock().unwrap() = Some(spawned);

        let err_text: Arc<Mutex<Vec<u8>>> = Arc::default();
        let err_thread = {
            let err_text = Arc::clone(&err_text);
            std::thread::spawn(move || {
                let mut b = Vec::new();
                let _ = stderr.read_to_end(&mut b);
                *err_text.lock().unwrap() = b;
            })
        };

        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()) {
                    let p = (us / 1e6 / total.max(1e-3)).clamp(0.0, 1.0) as f32;
                    if let ExportState::Running { progress, .. } = &mut *state.lock().unwrap() {
                        *progress = p;
                    }
                    ctx.request_repaint();
                }
            }
            let status = child.lock().unwrap().take().map(|mut c| c.wait());
            let _ = err_thread.join();
            let _ = std::fs::remove_file(&graph_file);
            let mut st = state.lock().unwrap();
            if matches!(*st, ExportState::Cancelled) {
                let _ = std::fs::remove_file(&settings.path);
            } else {
                *st = match status {
                    Some(Ok(s)) if s.success() => {
                        ExportState::Done { path: settings.path.clone(), seconds: started.elapsed().as_secs_f64() }
                    }
                    _ => {
                        let msg = error_line(&err_text.lock().unwrap()).unwrap_or_else(|| "FFmpeg หยุดทำงานกะทันหัน".to_owned());
                        let _ = std::fs::remove_file(&settings.path);
                        ExportState::Failed(msg)
                    }
                };
            }
            ctx.request_repaint();
        });
        job
    }

    pub fn state(&self) -> ExportState {
        self.state.lock().unwrap().clone()
    }

    pub fn cancel(&self) {
        *self.state.lock().unwrap() = ExportState::Cancelled;
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
}

fn encoder_args(s: &ExportSettings) -> Vec<String> {
    let crf = s.quality.crf(s.encoder, s.crf).clamp(0, 51).to_string();
    let v: Vec<&str> = match s.encoder {
        Encoder::Nvenc => {
            let name = if s.codec == Codec::H265 { "hevc_nvenc" } else { "h264_nvenc" };
            let preset = match s.speed {
                Speed::Fast => "p3",
                Speed::Balanced => "p5",
                Speed::Best => "p7",
            };
            vec!["-c:v", name, "-preset", preset, "-tune", "hq", "-rc", "vbr", "-cq", &crf, "-b:v", "0"]
        }
        Encoder::Cpu => {
            let name = if s.codec == Codec::H265 { "libx265" } else { "libx264" };
            let preset = match s.speed {
                Speed::Fast => "veryfast",
                Speed::Balanced => "medium",
                Speed::Best => "slow",
            };
            vec!["-c:v", name, "-preset", preset, "-crf", &crf]
        }
    };
    let mut out: Vec<String> = v.into_iter().map(str::to_owned).collect();
    out.extend(["-pix_fmt", "yuv420p"].map(str::to_owned));
    match s.codec {
        Codec::H264 => out.extend(["-profile:v", "high"].map(str::to_owned)),
        // hvc1 = ให้ QuickTime/iPhone เปิดได้
        Codec::H265 => out.extend(["-profile:v", "main", "-tag:v", "hvc1"].map(str::to_owned)),
    }
    out
}
