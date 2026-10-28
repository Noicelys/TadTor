//! ซับไตเติลอัตโนมัติด้วย whisper.cpp (ถอดเสียงพูดเป็นข้อความ ทำงานในเครื่อง ไม่ต้องใช้อินเทอร์เน็ตหลังดาวน์โหลด)
//!
//! ขั้นตอน: (ดาวน์โหลดโปรแกรม/โมเดลครั้งแรก) → เรนเดอร์เสียงจากไทม์ไลน์ → แบ่งก้อนตามช่วงที่มีเสียง
//! → whisper-cli (ทุกก้อนในรอบเดียว) → อ่าน .srt ของแต่ละก้อน → แบ่งบรรทัด

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;

use crate::core::speech;
use crate::core::subtitles::parse_srt;
use crate::core::timeline::Timeline;
use crate::ffmpeg::graph;
use crate::ffmpeg::{command, probe::error_line};

const CLI_URL: &str = "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip";
const CLI_ZIP_SIZE: u64 = 8_928_640;
pub const MODEL_NAME: &str = "ggml-large-v3-turbo-q5_0.bin";
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin";
const MODEL_SIZE: u64 = 574_041_195;
/// โมเดลตรวจเสียงพูดของรุ่นก่อน (ไม่ใช้แล้ว แต่อาจค้างอยู่ในโฟลเดอร์ ห้ามเข้าใจผิดว่าเป็นโมเดลภาษา)
const VAD_NAME: &str = "ggml-silero-v5.1.2.bin";
/// ขนาดดาวน์โหลดทั้งหมดโดยประมาณ (MB) ไว้บอกผู้ใช้
pub const DOWNLOAD_MB: u64 = (CLI_ZIP_SIZE + MODEL_SIZE) / 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Language {
    Thai,
    English,
    Japanese,
    Chinese,
    Korean,
    Auto,
}

impl Language {
    pub const ALL: [Language; 6] =
        [Language::Thai, Language::English, Language::Japanese, Language::Chinese, Language::Korean, Language::Auto];

    pub fn label(self) -> &'static str {
        match self {
            Language::Thai => "ไทย",
            Language::English => "อังกฤษ",
            Language::Japanese => "ญี่ปุ่น",
            Language::Chinese => "จีน",
            Language::Korean => "เกาหลี",
            Language::Auto => "ตรวจเอง",
        }
    }

    fn code(self) -> &'static str {
        match self {
            Language::Thai => "th",
            Language::English => "en",
            Language::Japanese => "ja",
            Language::Chinese => "zh",
            Language::Korean => "ko",
            Language::Auto => "auto",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TranscribeOptions {
    pub language: Language,
    /// รวมเสียงจากแทร็กเสียงด้วย (ปกติปิดไว้ เพลงทำให้ถอดเสียงพลาด)
    pub include_music: bool,
    /// ช่วงเวลาบนไทม์ไลน์ที่จะถอด (ผลลัพธ์เป็นเวลาบนไทม์ไลน์)
    pub from: f64,
    pub until: f64,
    /// ตัวอักษรต่อบรรทัดสูงสุด (0 = ไม่จำกัด)
    pub max_len: u32,
    /// คำเฉพาะ/ชื่อที่พูดถึง ช่วยให้สะกดถูก (ส่งเป็น prompt ของ whisper)
    pub prompt: String,
}

/// ที่เก็บ whisper: โฟลเดอร์ `whisper` ข้างโปรแกรม หรือในโฟลเดอร์โปรเจกต์ (ตอน cargo run)
fn base_dirs() -> Vec<PathBuf> {
    let mut v = vec![install_dir()];
    if let Ok(d) = std::env::current_dir() {
        v.push(d.join("whisper"));
    }
    v
}

fn find_cli() -> Option<PathBuf> {
    base_dirs().into_iter().find_map(|b| {
        [b.join("bin").join("Release").join("whisper-cli.exe"), b.join("bin").join("whisper-cli.exe")].into_iter().find(|p| p.is_file())
    })
}

fn find_model() -> Option<PathBuf> {
    base_dirs().into_iter().find_map(|b| {
        let preferred = b.join(MODEL_NAME);
        if preferred.is_file() {
            return Some(preferred);
        }
        // ใช้โมเดลอื่นที่ผู้ใช้วางไว้เองก็ได้ (ggml-*.bin)
        std::fs::read_dir(&b).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("ggml-") && n.ends_with(".bin") && n != VAD_NAME)
        })
    })
}

/// มีโปรแกรมและโมเดลพร้อมแล้วหรือยัง
pub fn ready() -> bool {
    find_cli().is_some() && find_model().is_some()
}

/// ที่จะติดตั้งถ้ายังไม่มี: ข้างไฟล์โปรแกรม
/// ยกเว้นตอนรันจาก target\ ของ cargo ให้ไปไว้ในโฟลเดอร์โปรเจกต์ ไม่งั้น cargo clean จะลบทิ้ง
fn install_dir() -> PathBuf {
    crate::paths::app_dir().join("whisper")
}

#[derive(Clone, Debug)]
pub enum TranscribeState {
    Downloading { what: &'static str, progress: f32 },
    Rendering,
    Running { progress: f32 },
    Done(Vec<(f64, f64, String)>),
    Failed(String),
    Cancelled,
}

pub struct TranscribeJob {
    pub state: Arc<Mutex<TranscribeState>>,
    cancel: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

impl TranscribeJob {
    pub fn start(ffmpeg: PathBuf, timeline: &Timeline, opts: TranscribeOptions, ctx: egui::Context) -> Self {
        let state = Arc::new(Mutex::new(TranscribeState::Rendering));
        let cancel = Arc::new(AtomicBool::new(false));
        let child: Arc<Mutex<Option<Child>>> = Arc::default();
        let job = Self { state: Arc::clone(&state), cancel: Arc::clone(&cancel), child: Arc::clone(&child) };
        let mut tl = timeline.clone();
        tl.main_muted = false;
        tl.audio_muted = !opts.include_music;

        std::thread::spawn(move || {
            let set = |s: TranscribeState| {
                *state.lock().unwrap() = s;
                ctx.request_repaint();
            };
            let result = run(&ffmpeg, &tl, opts, &set, &cancel, &child);
            if cancel.load(Ordering::Relaxed) {
                set(TranscribeState::Cancelled);
            } else {
                match result {
                    Ok(items) => set(TranscribeState::Done(items)),
                    Err(e) => set(TranscribeState::Failed(e)),
                }
            }
        });
        job
    }

    pub fn state(&self) -> TranscribeState {
        self.state.lock().unwrap().clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
}

type Setter<'a> = &'a dyn Fn(TranscribeState);

fn run(
    ffmpeg: &Path,
    tl: &Timeline,
    opts: TranscribeOptions,
    set: Setter,
    cancel: &AtomicBool,
    child: &Mutex<Option<Child>>,
) -> Result<Vec<(f64, f64, String)>, String> {
    // ---- 1) ดาวน์โหลดครั้งแรก ----
    let dir = install_dir();
    if find_cli().is_none() {
        let zip = dir.join("whisper-bin-x64.zip");
        download(CLI_URL, &zip, CLI_ZIP_SIZE, "โปรแกรมถอดเสียง", set, cancel, child)?;
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        // tar.exe ของ Windows แตกไฟล์ zip ได้
        let ok =
            command(Path::new(r"C:\Windows\System32\tar.exe")).arg("-xf").arg(&zip).arg("-C").arg(&bin).status().is_ok_and(|s| s.success());
        let _ = std::fs::remove_file(&zip);
        if !ok {
            return Err("แตกไฟล์ whisper ไม่สำเร็จ".to_owned());
        }
    }
    if find_model().is_none() {
        download(MODEL_URL, &dir.join(MODEL_NAME), MODEL_SIZE, "โมเดลภาษา", set, cancel, child)?;
    }
    let cli = find_cli().ok_or("หา whisper-cli.exe ไม่เจอหลังติดตั้ง")?;
    let model = find_model().ok_or("หาไฟล์โมเดลไม่เจอ")?;
    let model_dir = model.parent().map(Path::to_path_buf).unwrap_or_default();
    check_cancel(cancel)?;

    // ---- 2) เรนเดอร์เสียงจากไทม์ไลน์เป็น WAV 16kHz โมโน ----
    set(TranscribeState::Rendering);
    let from = opts.from.max(0.0);
    let until = opts.until.min(tl.duration());
    if until - from <= 0.1 {
        return Err("ช่วงที่เลือกไม่มีเสียง".to_owned());
    }
    // whisper-cli อ่านที่อยู่ไฟล์ภาษาไทยไม่ได้ จึงรันในโฟลเดอร์โมเดลแล้วส่งแค่ชื่อไฟล์ (ภาษาอังกฤษล้วน)
    let model_name = model.file_name().ok_or("ชื่อไฟล์โมเดลผิด")?.to_owned();
    let (rate, samples) = render_audio(ffmpeg, tl, from, until)?;
    check_cancel(cancel)?;

    // ---- 3) แบ่งเป็นก้อนตามช่วงที่มีเสียง ----
    // ไม่ใช้ --vad ของ whisper เพราะเวลาของซับคลาดหลายวินาทีหลังช่วงเงียบ และส่งเฉพาะช่วงที่มีเสียง whisper จึงไม่แต่งคำมั่วตอนเงียบ
    let regions = speech::speech_regions(&samples, rate, speech::VadSettings::default());
    let chunks = speech::group_chunks(&regions, 1.0, 25.0);
    if chunks.is_empty() {
        return Ok(Vec::new());
    }
    let names: Vec<String> = (0..chunks.len()).map(|i| format!("chunk{i:03}.wav")).collect();
    for (name, &(a, b)) in names.iter().zip(&chunks) {
        let (s0, s1) = ((a * rate as f64) as usize, ((b * rate as f64) as usize).min(samples.len()));
        std::fs::write(model_dir.join(name), speech::wav_s16(rate, &samples[s0..s1])).map_err(|e| e.to_string())?;
    }
    let cleanup = || {
        for name in &names {
            let _ = std::fs::remove_file(model_dir.join(name));
            let _ = std::fs::remove_file(model_dir.join(format!("{name}.srt")));
        }
    };

    // ---- 4) ถอดเสียงทุกก้อนในรอบเดียว (โหลดโมเดลครั้งเดียว) ----
    set(TranscribeState::Running { progress: 0.0 });
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let mut c = command(&cli);
    c.current_dir(&model_dir).arg("-m").arg(&model_name);
    for name in &names {
        c.args(["-f", name]);
    }
    c.args(["-l", opts.language.code(), "-osrt", "-pp", "-sns", "-t", &threads.to_string()]);
    if !opts.prompt.trim().is_empty() {
        c.arg("--prompt").arg(opts.prompt.trim());
    }
    let mut c = match c.stdout(Stdio::null()).stderr(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => {
            cleanup();
            return Err(format!("เรียก whisper ไม่ได้: {e}"));
        }
    };
    let stderr = c.stderr.take().unwrap();
    *child.lock().unwrap() = Some(c);
    // ความคืบหน้ารวม = ความยาวก้อนที่เสร็จแล้ว + ก้อนปัจจุบันตามเปอร์เซ็นต์
    let lens: Vec<f64> = chunks.iter().map(|(a, b)| b - a).collect();
    let all: f64 = lens.iter().sum::<f64>().max(1e-3);
    let mut current: Option<usize> = None;
    let mut tail = Vec::new();
    for line in BufReader::new(stderr).split(b'\n').map_while(Result::ok) {
        let text = String::from_utf8_lossy(&line);
        if let Some(i) = names.iter().position(|n| text.contains(&format!("processing '{n}'"))) {
            current = Some(i);
        }
        if let Some(p) = text.split("progress =").nth(1).and_then(|r| r.trim().trim_end_matches('%').trim().parse::<f64>().ok()) {
            let i = current.unwrap_or(0);
            let done: f64 = lens[..i].iter().sum::<f64>() + lens[i] * p / 100.0;
            set(TranscribeState::Running { progress: (done / all) as f32 });
        }
        tail = line;
    }
    let status = child.lock().unwrap().take().map(|mut c| c.wait());
    if cancel.load(Ordering::Relaxed) {
        cleanup();
        return Err("ยกเลิกแล้ว".to_owned());
    }
    if !matches!(status, Some(Ok(s)) if s.success()) {
        cleanup();
        return Err(format!("ถอดเสียงไม่สำเร็จ: {}", String::from_utf8_lossy(&tail).trim()));
    }

    // ---- 5) รวมผล: เลื่อนเวลาตามก้อน แบ่งบรรทัด กระจายเวลาไปตามช่วงที่มีเสียงจริง ----
    let mut items: Vec<(f64, f64, String)> = Vec::new();
    for (name, &(a, b)) in names.iter().zip(&chunks) {
        let Ok(bytes) = std::fs::read(model_dir.join(format!("{name}.srt"))) else { continue };
        for (s, e, t) in parse_srt(&String::from_utf8_lossy(&bytes)) {
            let t = speech::clean_text(&t);
            if is_noise(&t) {
                continue;
            }
            let (s, e) = ((s + a).min(b), (e + a).min(b));
            if e - s < 0.05 {
                continue;
            }
            for (ls, le, lt) in speech::split_line(&t, s, e, opts.max_len as usize) {
                let (ls, le) = (speech::spread_over(&regions, s, e, ls), speech::spread_over(&regions, s, e, le));
                items.push((ls + from, (le + from).min(until), lt));
            }
        }
    }
    cleanup();
    Ok(speech::drop_repeats(items))
}

/// เรนเดอร์เสียงจากไทม์ไลน์ช่วง [from, until) เป็น PCM 16kHz โมโน
pub fn render_audio(ffmpeg: &Path, tl: &Timeline, from: f64, until: f64) -> Result<(u32, Vec<i16>), String> {
    let g = graph::build(tl, from, until, None, Some(16_000));
    let out = command(ffmpeg)
        .current_dir(&g.cwd)
        .args(["-y", "-v", "error"])
        .args(&g.inputs)
        .arg("-filter_complex")
        .arg(&g.filter)
        .args(["-map", "[aout]", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", "-f", "wav", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(error_line(&out.stderr).unwrap_or_else(|| "เรนเดอร์เสียงไม่สำเร็จ".to_owned()));
    }
    speech::read_wav_s16(&out.stdout).ok_or_else(|| "อ่านเสียงที่เรนเดอร์ไม่ได้".to_owned())
}

/// whisper ชอบใส่ข้อความแบบ [เพลง] (เสียงดนตรี) ตอนไม่มีคนพูด ตัดทิ้ง
fn is_noise(t: &str) -> bool {
    let t = t.trim();
    t.is_empty() || (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')'))
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("ยกเลิกแล้ว".to_owned())
    } else {
        Ok(())
    }
}

/// ดาวน์โหลดด้วย curl.exe ของ Windows (ดูความคืบหน้าจากขนาดไฟล์)
fn download(
    url: &str,
    dest: &Path,
    size: u64,
    what: &'static str,
    set: Setter,
    cancel: &AtomicBool,
    child: &Mutex<Option<Child>>,
) -> Result<(), String> {
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    let part = dest.with_extension("part");
    set(TranscribeState::Downloading { what, progress: 0.0 });
    let c = command(Path::new(r"C:\Windows\System32\curl.exe"))
        .args(["-L", "--fail", "-s", "-S", "-C", "-", "-o"])
        .arg(&part)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("เรียก curl ไม่ได้: {e}"))?;
    *child.lock().unwrap() = Some(c);
    loop {
        let done = child.lock().unwrap().as_mut().and_then(|c| c.try_wait().ok().flatten());
        let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        set(TranscribeState::Downloading { what, progress: (have as f32 / size as f32).min(1.0) });
        if let Some(status) = done {
            let mut err = String::new();
            if let Some(mut c) = child.lock().unwrap().take() {
                if let Some(mut e) = c.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
            }
            check_cancel(cancel)?;
            if !status.success() {
                return Err(format!("ดาวน์โหลด{what}ไม่สำเร็จ (เช็กอินเทอร์เน็ต): {}", err.trim()));
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}
