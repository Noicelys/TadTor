//! งานเบื้องหลังของคลังสื่อ: อ่านข้อมูลไฟล์ → ภาพย่อ → รูปคลื่นเสียง
//! ทำทีละไม่กี่ไฟล์พร้อมกัน หน้าจอไม่ค้างแม้นำเข้าทีละเยอะ ๆ

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::core::media::{MediaInfo, MediaKind};
use crate::ffmpeg::{probe, FfmpegProbe};

const WORKERS: usize = 3;

pub enum MediaEvent {
    Probed(PathBuf, Result<MediaInfo, String>),
    Thumbnail(PathBuf, egui::ColorImage),
    Waveform(PathBuf, Vec<f32>),
}

pub struct MediaJobs {
    tx: Sender<(PathBuf, MediaKind)>,
    rx: Receiver<MediaEvent>,
}

impl MediaJobs {
    pub fn start(ctx: egui::Context, ffmpeg: FfmpegProbe) -> Self {
        let (tx, job_rx) = mpsc::channel::<(PathBuf, MediaKind)>();
        let (event_tx, rx) = mpsc::channel();
        let job_rx = Arc::new(Mutex::new(job_rx));
        for _ in 0..WORKERS {
            let job_rx = Arc::clone(&job_rx);
            let event_tx = event_tx.clone();
            let ctx = ctx.clone();
            let ffmpeg = ffmpeg.clone();
            std::thread::spawn(move || loop {
                let job = job_rx.lock().unwrap().recv();
                let Ok((path, kind)) = job else { return };
                run(&ffmpeg, path, kind, &event_tx, &ctx);
            });
        }
        Self { tx, rx }
    }

    pub fn enqueue(&self, path: PathBuf, kind: MediaKind) {
        let _ = self.tx.send((path, kind));
    }

    pub fn poll(&self) -> Vec<MediaEvent> {
        self.rx.try_iter().collect()
    }
}

fn run(ffmpeg: &FfmpegProbe, path: PathBuf, kind: MediaKind, tx: &Sender<MediaEvent>, ctx: &egui::Context) {
    let send = |e: MediaEvent| {
        let _ = tx.send(e);
        ctx.request_repaint();
    };
    let Some(tools) = ffmpeg.wait_info() else {
        send(MediaEvent::Probed(path, Err("ไม่พบ FFmpeg".to_owned())));
        return;
    };
    let Some(ffprobe) = tools.ffprobe.as_ref() else {
        send(MediaEvent::Probed(path, Err("ไม่พบ ffprobe".to_owned())));
        return;
    };
    let info = probe::probe(ffprobe, &path, kind);
    let ok = info.clone().ok();
    send(MediaEvent::Probed(path.clone(), info));
    let Some(info) = ok else { return };

    if kind.is_visual() {
        if let Ok((w, h, px)) = probe::thumbnail(&tools.ffmpeg, &path, kind, &info) {
            let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
            send(MediaEvent::Thumbnail(path.clone(), img));
        }
    }
    if kind == MediaKind::Audio {
        if let Ok(peaks) = probe::waveform(&tools.ffmpeg, &path) {
            send(MediaEvent::Waveform(path, peaks));
        }
    }
}
