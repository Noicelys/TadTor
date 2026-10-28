//! เล่นพรีวิว: FFmpeg สองตัว ตัวหนึ่งเรนเดอร์ภาพ อีกตัวเรนเดอร์เสียงของไทม์ไลน์ตั้งแต่จุดที่กดเล่น
//! และตัวดึงภาพนิ่งทีละเฟรมตอนเลื่อนหัวเล่น (ตอนไม่ได้เล่น)

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, ColorImage};

use super::audio::{AudioOut, AudioSource};
use crate::core::timeline::Timeline;
use crate::ffmpeg::graph::{self, VideoSpec};
use crate::ffmpeg::{command, probe::last_line};

/// พรีวิวเปิดไฟล์พร้อมกันได้เท่านี้ ถ้าเกินจะเล่นต่อเป็นช่วง ๆ
const MAX_PREVIEW_INPUTS: usize = 16;
/// เก็บเสียงล่วงหน้าไว้ไม่เกินเท่านี้ (วินาที)
const AUDIO_AHEAD: f64 = 0.5;

type SharedChild = Arc<Mutex<Option<Child>>>;

fn spawn_ffmpeg(ffmpeg: &Path, g: &graph::Graph, output: &[&str]) -> std::io::Result<Child> {
    command(ffmpeg)
        .current_dir(&g.cwd)
        .args(["-v", "error", "-nostdin"])
        .args(&g.inputs)
        .arg("-filter_complex")
        .arg(&g.filter)
        .args(output)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

fn kill(child: &SharedChild) {
    if let Some(mut c) = child.lock().unwrap().take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

pub struct PlayerUpdate {
    pub time: f64,
    pub frame: Option<ColorImage>,
    pub finished: bool,
}

pub struct Player {
    pub from: f64,
    pub until: f64,
    fps: f64,
    frames: Receiver<(u64, ColorImage)>,
    next: Option<(u64, ColorImage)>,
    got_first_frame: bool,
    created: Instant,
    /// นาฬิกาสำรองเมื่อไม่มีเสียง: เริ่มนับตอนได้ภาพแรก
    wall_start: Option<Instant>,
    audio: Option<Arc<AudioSource>>,
    sample_rate: f64,
    stop: Arc<AtomicBool>,
    children: Vec<SharedChild>,
    pub error: Arc<Mutex<Option<String>>>,
}

impl Player {
    pub fn start(ffmpeg: &Path, tl: &Timeline, from: f64, spec: VideoSpec, audio_out: Option<&AudioOut>) -> Self {
        let until = graph::preview_until(tl, from, MAX_PREVIEW_INPUTS);
        let stop = Arc::new(AtomicBool::new(false));
        let error: Arc<Mutex<Option<String>>> = Arc::default();
        let mut children = Vec::new();

        // ---- ภาพ ----
        let (tx, frames) = mpsc::sync_channel(4);
        let g = graph::build(tl, from, until, Some(spec), None);
        let video_child: SharedChild = Arc::default();
        match spawn_ffmpeg(ffmpeg, &g, &["-map", "[vout]", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]) {
            Ok(mut child) => {
                let stdout = child.stdout.take().unwrap();
                watch_stderr(&mut child, Arc::clone(&error));
                *video_child.lock().unwrap() = Some(child);
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || read_frames(stdout, spec, tx, stop));
            }
            Err(e) => *error.lock().unwrap() = Some(format!("เริ่มพรีวิวไม่ได้: {e}")),
        }
        children.push(video_child);

        // ---- เสียง ----
        let mut audio = None;
        let mut sample_rate = 48_000.0;
        if let Some(out) = audio_out {
            sample_rate = out.sample_rate as f64;
            let g = graph::build(tl, from, until, None, Some(out.sample_rate));
            let rate = out.sample_rate.to_string();
            if let Ok(mut child) = spawn_ffmpeg(ffmpeg, &g, &["-map", "[aout]", "-f", "f32le", "-ac", "2", "-ar", &rate, "pipe:1"]) {
                let source = Arc::new(AudioSource::default());
                source.hold.store(true, Ordering::Relaxed);
                let stdout = child.stdout.take().unwrap();
                watch_stderr(&mut child, Arc::clone(&error));
                let audio_child: SharedChild = Arc::new(Mutex::new(Some(child)));
                children.push(audio_child);
                let (src, stop) = (Arc::clone(&source), Arc::clone(&stop));
                let max_samples = (out.sample_rate as f64 * 2.0 * AUDIO_AHEAD) as usize;
                std::thread::spawn(move || read_audio(stdout, src, stop, max_samples));
                out.set_source(Some(Arc::clone(&source)));
                audio = Some(source);
            }
        }

        Self {
            from,
            until,
            fps: spec.fps as f64,
            frames,
            next: None,
            got_first_frame: false,
            created: Instant::now(),
            wall_start: None,
            audio,
            sample_rate,
            stop,
            children,
            error,
        }
    }

    fn clock(&self) -> f64 {
        let t = match (&self.audio, self.wall_start) {
            (Some(src), _) => src.played.load(Ordering::Relaxed) as f64 / self.sample_rate,
            (None, Some(start)) => start.elapsed().as_secs_f64(),
            (None, None) => 0.0,
        };
        (self.from + t).min(self.until)
    }

    pub fn update(&mut self) -> PlayerUpdate {
        // รอภาพแรกก่อนค่อยปล่อยเสียง จะได้เริ่มพร้อมกัน (รอไม่เกิน 2 วินาที)
        if !self.got_first_frame {
            if self.next.is_none() {
                self.next = self.frames.try_recv().ok();
            }
            let timeout = self.created.elapsed() > Duration::from_secs(2);
            if self.next.is_some() || timeout {
                self.got_first_frame = true;
                self.wall_start = Some(Instant::now());
                if let Some(src) = &self.audio {
                    src.hold.store(false, Ordering::Relaxed);
                }
            }
        }

        let time = self.clock();
        let mut frame = None;
        loop {
            if self.next.is_none() {
                self.next = self.frames.try_recv().ok();
            }
            match &self.next {
                Some((n, _)) if self.from + *n as f64 / self.fps <= time + 0.5 / self.fps => {
                    frame = self.next.take().map(|(_, img)| img);
                }
                _ => break,
            }
        }

        let finished = match &self.audio {
            Some(src) => src.eof.load(Ordering::Relaxed) && src.buf.lock().unwrap().is_empty(),
            None => time >= self.until - 1e-3,
        };
        PlayerUpdate { time, frame, finished }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for c in &self.children {
            kill(c);
        }
    }
}

fn watch_stderr(child: &mut Child, error: Arc<Mutex<Option<String>>>) {
    if let Some(mut err) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = err.read_to_end(&mut bytes);
            if let Some(line) = last_line(&bytes) {
                error.lock().unwrap().get_or_insert(line);
            }
        });
    }
}

fn read_frames(mut stdout: impl Read, spec: VideoSpec, tx: SyncSender<(u64, ColorImage)>, stop: Arc<AtomicBool>) {
    let size = [spec.width as usize, spec.height as usize];
    let mut buf = vec![0u8; size[0] * size[1] * 4];
    let mut n = 0u64;
    while !stop.load(Ordering::Relaxed) && stdout.read_exact(&mut buf).is_ok() {
        let img = ColorImage::from_rgba_unmultiplied(size, &buf);
        if tx.send((n, img)).is_err() {
            break;
        }
        n += 1;
    }
}

fn read_audio(mut stdout: impl Read, src: Arc<AudioSource>, stop: Arc<AtomicBool>, max_samples: usize) {
    let mut buf = vec![0u8; 16 * 1024];
    let mut leftover: Vec<u8> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let n = match stdout.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        leftover.extend_from_slice(&buf[..n]);
        let whole = leftover.len() / 4 * 4;
        {
            let mut q = src.buf.lock().unwrap();
            q.extend(leftover[..whole].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)));
        }
        leftover.drain(..whole);
        while !stop.load(Ordering::Relaxed) && src.buf.lock().unwrap().len() > max_samples {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    src.eof.store(true, Ordering::Relaxed);
}

// ---------------- ภาพนิ่งตอนเลื่อนหัวเล่น ----------------

struct GrabRequest {
    id: u64,
    ffmpeg: PathBuf,
    timeline: Timeline,
    time: f64,
    spec: VideoSpec,
}

/// ดึงภาพทีละเฟรมในเธรดเบื้องหลัง ถ้ามีคำขอใหม่เข้ามาระหว่างทำ จะข้ามคำขอเก่าไปทำอันล่าสุดเลย
pub struct FrameGrabber {
    request: Arc<(Mutex<Option<GrabRequest>>, Condvar)>,
    result: Arc<Mutex<Option<(u64, ColorImage)>>>,
}

impl FrameGrabber {
    pub fn start(ctx: egui::Context) -> Self {
        let request: Arc<(Mutex<Option<GrabRequest>>, Condvar)> = Arc::default();
        let result: Arc<Mutex<Option<(u64, ColorImage)>>> = Arc::default();
        let (req, res) = (Arc::clone(&request), Arc::clone(&result));
        std::thread::spawn(move || loop {
            let job = {
                let (lock, cvar) = &*req;
                let mut slot = lock.lock().unwrap();
                while slot.is_none() {
                    slot = cvar.wait(slot).unwrap();
                }
                slot.take().unwrap()
            };
            if let Some(img) = grab(&job) {
                *res.lock().unwrap() = Some((job.id, img));
                ctx.request_repaint();
            }
        });
        Self { request, result }
    }

    pub fn request(&self, id: u64, ffmpeg: PathBuf, timeline: Timeline, time: f64, spec: VideoSpec) {
        let (lock, cvar) = &*self.request;
        *lock.lock().unwrap() = Some(GrabRequest { id, ffmpeg, timeline, time, spec });
        cvar.notify_one();
    }

    pub fn take(&self) -> Option<(u64, ColorImage)> {
        self.result.lock().unwrap().take()
    }
}

fn grab(job: &GrabRequest) -> Option<ColorImage> {
    let g = graph::build(&job.timeline, job.time, job.time + 0.5, Some(job.spec), None);
    let mut child =
        spawn_ffmpeg(&job.ffmpeg, &g, &["-map", "[vout]", "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]).ok()?;
    let mut stdout = child.stdout.take()?;
    drop(child.stderr.take());
    let size = [job.spec.width as usize, job.spec.height as usize];
    let mut buf = vec![0u8; size[0] * size[1] * 4];
    let ok = stdout.read_exact(&mut buf).is_ok();
    let _ = child.kill();
    let _ = child.wait();
    ok.then(|| ColorImage::from_rgba_unmultiplied(size, &buf))
}
