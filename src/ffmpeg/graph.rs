//! แปลงไทม์ไลน์เป็นคำสั่ง FFmpeg (filter graph)
//!
//! ใช้ทางเดียวกันทั้งพรีวิวและส่งออก ภาพที่เห็นตอนพรีวิวจึงตรงกับไฟล์ที่ได้จริง
//! ผลลัพธ์มีป้าย `[vout]` (ภาพ) และ/หรือ `[aout]` (เสียงสเตอริโอ) ให้ผู้เรียกเอาไป map ต่อ
//!
//! ข้อความ ฟอนต์ และไฟล์ LUT ถูกคัดลอกไปไว้ในโฟลเดอร์ทำงาน (`work_dir`) แล้วอ้างด้วยชื่อสั้น ๆ
//! เพราะการใส่ข้อความ/ที่อยู่ไฟล์ Windows ลงใน filter graph ตรง ๆ ต้อง escape ยุ่งยากมาก
//! ผู้เรียกจึงต้องรัน FFmpeg โดยตั้ง current_dir เป็น `Graph::cwd`

use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::core::fx::{ClipFx, Effect, FilterPreset, Fit, MotionNum, Rgba, TextAlign, TextStyle, TransitionKind, Voice};
use crate::core::media::MediaKind;
use crate::core::timeline::{Clip, Timeline, TrackKind};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

pub struct Graph {
    /// อาร์กิวเมนต์ของไฟล์ขาเข้า (-ss … -i …)
    pub inputs: Vec<OsString>,
    /// ข้อความ filter graph (ใส่ใน -filter_complex)
    pub filter: String,
    /// โฟลเดอร์ที่ต้องรัน FFmpeg (มีไฟล์ข้อความ/ฟอนต์/LUT ที่ graph อ้างถึง)
    pub cwd: PathBuf,
}

/// โฟลเดอร์ทำงานชั่วคราวของโปรแกรม
pub fn work_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("tadtor-work");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// เขียนไฟล์ลงโฟลเดอร์ทำงานโดยตั้งชื่อตามเนื้อหา (เนื้อหาเดิม = ชื่อเดิม ไม่ต้องเขียนซ้ำ)
fn stash(prefix: &str, ext: &str, bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let name = format!("{prefix}{:016x}.{ext}", h.finish());
    let path = work_dir().join(&name);
    if !path.exists() {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
    name
}

/// ฟอนต์ภาษาไทยของ Windows (ตัวธรรมดา, ตัวหนา) คัดลอกมาไว้ในโฟลเดอร์ทำงาน
fn font_file(bold: bool) -> Option<String> {
    let candidates: &[&str] = if bold {
        &[r"C:\Windows\Fonts\LeelaUIb.ttf", r"C:\Windows\Fonts\leelawdb.ttf", r"C:\Windows\Fonts\tahomabd.ttf"]
    } else {
        &[r"C:\Windows\Fonts\LeelawUI.ttf", r"C:\Windows\Fonts\leelawad.ttf", r"C:\Windows\Fonts\tahoma.ttf"]
    };
    let name = if bold { "font-bold.ttf" } else { "font.ttf" };
    let dest = work_dir().join(name);
    if dest.exists() {
        return Some(name.to_owned());
    }
    let src = candidates.iter().map(Path::new).find(|p| p.is_file())?;
    std::fs::copy(src, &dest).ok()?;
    Some(name.to_owned())
}

/// ช่วงของคลิปที่อยู่ในหน้าต่างเวลา [from, until)
struct Seg<'a> {
    clip: &'a Clip,
    /// เวลาเริ่มนับจาก `from`
    offset: f64,
    len: f64,
    /// จุดเริ่มในไฟล์ต้นฉบับ
    src: f64,
}

impl Seg<'_> {
    /// เวลาในคลิป (นับจากหัวคลิป) ตรงจุดเริ่มของช่วงนี้
    fn clip_offset(&self) -> f64 {
        self.src - self.clip.src_in
    }
}

fn segments(clips: &[Clip], from: f64, until: f64) -> Vec<Seg<'_>> {
    clips
        .iter()
        .filter_map(|c| {
            let s = c.start.max(from);
            let e = c.end().min(until);
            (e - s > 1e-4).then_some(Seg { clip: c, offset: s - from, len: e - s, src: c.src_in + (s - c.start) })
        })
        .collect()
}

fn num(v: f64) -> String {
    format!("{:.6}", v.max(0.0))
}

fn color(c: Rgba) -> String {
    format!("0x{:02x}{:02x}{:02x}@{:.3}", c[0], c[1], c[2], c[3] as f32 / 255.0)
}

/// สีทึบ (ไม่สนช่องโปร่งใส)
fn solid(c: Rgba) -> String {
    format!("0x{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn push_input(inputs: &mut Vec<OsString>, seg: &Seg, fps: u32) {
    let mut a = |s: &str| inputs.push(s.into());
    match seg.clip.kind {
        MediaKind::Image => {
            let gif = seg.clip.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"));
            if gif {
                a("-ignore_loop");
                a("0");
            } else {
                a("-loop");
                a("1");
                a("-framerate");
                a(&fps.to_string());
            }
        }
        _ => {
            a("-ss");
            a(&num(seg.src));
        }
    }
    a("-t");
    a(&num(seg.len));
    inputs.push("-i".into());
    inputs.push(seg.clip.path.clone().into_os_string());
}

/// เลือกเวลาสิ้นสุดให้พรีวิวเปิดไฟล์พร้อมกันไม่เกิน `max_inputs` ไฟล์ (เปิดเยอะจะเริ่มเล่นช้า)
pub fn preview_until(tl: &Timeline, from: f64, max_inputs: usize) -> f64 {
    let total = tl.duration();
    let mut starts: Vec<f64> =
        tl.main.iter().chain(&tl.overlay).chain(&tl.audio).filter(|c| c.end() > from + 1e-4).map(|c| c.start.max(from)).collect();
    starts.sort_by(f64::total_cmp);
    match starts.get(max_inputs) {
        Some(&s) if s > from + 0.5 => s.min(total),
        _ => total,
    }
}

/// ถ้าเวลา t อยู่กลางทรานซิชัน คืนช่วง [เริ่ม, จบ) ของทรานซิชันนั้น
fn transition_span(tl: &Timeline, t: f64) -> Option<(f64, f64)> {
    (0..tl.main.len()).find_map(|i| {
        let (_, d) = tl.transition_after(i)?;
        let end = tl.main[i].end();
        let start = end - d;
        (t > start + 1e-6 && t < end - 1e-6).then_some((start, end))
    })
}

// ---------------- ภาพ: ฟิลเตอร์ / ปรับสี / เอฟเฟกต์ ----------------

fn preset_filter(p: FilterPreset) -> Option<&'static str> {
    Some(match p {
        FilterPreset::None => return None,
        FilterPreset::Vivid => "eq=saturation=1.45:contrast=1.08",
        FilterPreset::Warm => "colortemperature=temperature=4300,eq=saturation=1.1",
        FilterPreset::Cool => "colortemperature=temperature=9500,eq=saturation=0.95",
        FilterPreset::Film => "curves=preset=medium_contrast,colorbalance=rs=0.08:bs=-0.06:rh=-0.04:bh=0.08,eq=saturation=0.9",
        FilterPreset::Vintage => "curves=preset=vintage,eq=saturation=0.85",
        FilterPreset::Fade => "curves=all='0/0.12 1/0.92',eq=saturation=0.8",
        FilterPreset::Sepia => "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131",
        FilterPreset::Mono => "hue=s=0",
        FilterPreset::Noir => "hue=s=0,curves=preset=strong_contrast",
    })
}

/// เอฟเฟกต์ตามความแรง `k` (1 = ปกติ, 0 = ไม่มีผล)
fn effect_filter(e: Effect, k: f32) -> Option<String> {
    let k = k.clamp(0.0, 2.0);
    if e == Effect::None || (k < 0.01 && e != Effect::Mirror) {
        return None;
    }
    let angle = |base: f32| (base * k).min(1.5);
    Some(match e {
        Effect::None => return None,
        Effect::Vignette => format!("vignette=angle={:.3}", angle(std::f32::consts::FRAC_PI_4)),
        Effect::Blur => format!("gblur=sigma={:.2}", 8.0 * k),
        Effect::Sharpen => format!("unsharp=5:5:{:.2}:5:5:0", 1.4 * k),
        Effect::Grain => format!("noise=alls={:.0}:allf=t+u", (18.0 * k).max(1.0)),
        Effect::OldFilm => format!(
            "curves=preset=vintage,noise=alls={:.0}:allf=t+u,vignette=angle={:.3}",
            (22.0 * k).max(1.0),
            angle(std::f32::consts::PI / 3.5)
        ),
        Effect::Cinema => {
            let bar = (0.11 * k).min(0.4);
            format!(
                "drawbox=x=0:y=0:w=iw:h=ih*{bar:.4}:color=black:t=fill,drawbox=x=0:y=ih*{:.4}:w=iw:h=ih*{bar:.4}:color=black:t=fill",
                1.0 - bar
            )
        }
        Effect::Mirror => "hflip".to_owned(),
        Effect::Glow => format!(
            "split[g0][g1];[g1]gblur=sigma={:.1}[g2];[g0][g2]blend=all_mode=screen:all_opacity={:.3}",
            10.0 + 4.0 * k,
            (0.55 * k).min(1.0)
        ),
    })
}

/// หมุนทีละ 90° / กลับด้าน
fn orient(fx: &ClipFx) -> Vec<String> {
    let mut v = Vec::new();
    match fx.rotate % 4 {
        1 => v.push("transpose=clock".to_owned()),
        2 => v.push("hflip,vflip".to_owned()),
        3 => v.push("transpose=cclock".to_owned()),
        _ => {}
    }
    if fx.flip_h {
        v.push("hflip".to_owned());
    }
    if fx.flip_v {
        v.push("vflip".to_owned());
    }
    v
}

/// ขนาดจอ × ซูม (ปัดเป็นเลขคู่)
fn zoomed(fx: &ClipFx, w: u32, h: u32) -> (u32, u32) {
    let z = fx.zoom.clamp(0.1, 5.0);
    let even = |x: f32| ((x / 2.0).round() as u32).max(1) * 2;
    (even(w as f32 * z), even(h as f32 * z))
}

/// หมุน/กลับด้าน + ย่อขยายตามแบบวาง + ครอปส่วนที่ล้นจอ
fn layout_pre(fx: &ClipFx, w: u32, h: u32) -> Vec<String> {
    let mut v = orient(fx);
    let z = fx.zoom.clamp(0.1, 5.0);
    let (zw, zh) = zoomed(fx, w, h);
    v.push(match fx.fit {
        Fit::Contain => format!("scale={zw}:{zh}:force_original_aspect_ratio=decrease:force_divisible_by=2"),
        Fit::Cover => format!("scale={zw}:{zh}:force_original_aspect_ratio=increase:force_divisible_by=2"),
        Fit::Stretch => format!("scale={zw}:{zh}"),
    });
    // ภาพอาจล้นจอ (ซูม/เต็มจอ) ครอปให้เหลือขนาดจอ เลื่อนตาม pan
    if z > 1.0 + 1e-3 || fx.fit == Fit::Cover {
        let cx = (1.0 - fx.pan_x.clamp(-1.0, 1.0)) / 2.0;
        let cy = (1.0 - fx.pan_y.clamp(-1.0, 1.0)) / 2.0;
        v.push(format!("crop=w='min(iw,{w})':h='min(ih,{h})':x='(iw-ow)*{cx:.4}':y='(ih-oh)*{cy:.4}'"));
    }
    v
}

/// เติมพื้นที่ว่างรอบภาพจนเต็มจอ (ภาพเล็กกว่าจอวางตาม pan)
fn layout_pad(fx: &ClipFx, w: u32, h: u32) -> String {
    let px = (1.0 + fx.pan_x.clamp(-1.0, 1.0)) / 2.0;
    let py = (1.0 + fx.pan_y.clamp(-1.0, 1.0)) / 2.0;
    format!("pad={w}:{h}:x='(ow-iw)*{px:.4}':y='(oh-ih)*{py:.4}':color={}", solid(fx.background))
}

/// ฟิลเตอร์ปรับสีของคลิป (ใช้กับภาพก่อนเติมขอบดำ)
fn color_chain(fx: &ClipFx, lut: Option<&str>) -> Vec<String> {
    let mut v = Vec::new();
    if fx.brightness.abs() > 1e-3 || fx.contrast.abs() > 1e-3 || fx.saturation.abs() > 1e-3 {
        v.push(format!(
            "eq=brightness={:.3}:contrast={:.3}:saturation={:.3}",
            fx.brightness * 0.25,
            1.0 + fx.contrast * 0.5,
            (1.0 + fx.saturation).max(0.0)
        ));
    }
    if fx.temperature.abs() > 1e-3 {
        // ค่าบวก = อุ่นขึ้น (อุณหภูมิสีต่ำลง)
        v.push(format!("colortemperature=temperature={:.0}", 6500.0 - fx.temperature as f64 * 3000.0));
    }
    if fx.tint.abs() > 1e-3 {
        // ค่าบวก = ม่วงแดง (ลดเขียว)
        let g = -fx.tint * 0.15;
        v.push(format!("colorbalance=gs={g:.3}:gm={g:.3}:gh={g:.3}"));
    }
    if fx.hue.abs() > 1e-3 {
        v.push(format!("hue=h={:.1}", fx.hue * 180.0));
    }
    if fx.highlights.abs() > 1e-3 || fx.shadows.abs() > 1e-3 || fx.fade > 1e-3 {
        // เส้นโค้งเดียว: จุดดำ (จาง) / ส่วนมืด / ส่วนสว่าง
        let black = fx.fade.clamp(0.0, 1.0) * 0.25;
        let low = (0.25 + fx.shadows.clamp(-1.0, 1.0) * 0.12).max(black + 0.02);
        let high = 0.75 + fx.highlights.clamp(-1.0, 1.0) * 0.12;
        let white = 1.0 - fx.fade.clamp(0.0, 1.0) * 0.06;
        v.push(format!("curves=all='0/{black:.3} 0.25/{low:.3} 0.75/{high:.3} 1/{white:.3}'"));
    }
    if let Some(l) = lut {
        v.push(format!("lut3d=file={l}"));
    }
    if fx.sharpen > 1e-3 {
        v.push(format!("unsharp=5:5:{:.2}:5:5:0", fx.sharpen * 2.0));
    }
    if fx.grain > 1e-3 {
        v.push(format!("noise=alls={:.0}:allf=t+u", (fx.grain.min(1.0) * 40.0).max(1.0)));
    }
    v
}

/// ตัดช่วงของคลิปจากไฟล์ขาเข้า ให้เวลาเริ่มที่ 0 และเฟรมเรตตรงกับงาน
fn clip_start(seg: &Seg, fps: u32) -> Vec<String> {
    vec![format!("trim=duration={}", num(seg.len)), "setpts=PTS-STARTPTS".to_owned(), format!("fps={fps}")]
}

/// ต่อสาย filter ภาพของคลิปหนึ่ง จาก `[k:v:0]` ไปเป็น `[out]`
fn video_chain(chains: &mut Vec<String>, k: usize, seg: &Seg, spec: VideoSpec, out: &str) {
    let fx = &seg.clip.fx;
    let (w, h, fps) = (spec.width, spec.height, spec.fps);
    let mut pre = clip_start(seg, fps);
    pre.extend(layout_pre(fx, w, h));
    let post = vec![layout_pad(fx, w, h), "setsar=1".to_owned()];
    look_chain(chains, &format!("c{k}"), format!("[{k}:v:0]"), pre, post, &format!("format=yuv420p,settb=1/{fps}"), fx, out);
}

/// ฟิลเตอร์ / ปรับสี / ขอบมืด / เอฟเฟกต์ ของคลิป: `pre` ทำก่อนปรับสี, `post` ทำหลังปรับสีก่อนเอฟเฟกต์, `finish` ปิดท้าย
#[allow(clippy::too_many_arguments)]
fn look_chain(
    chains: &mut Vec<String>,
    tag: &str,
    input: String,
    mut pre: Vec<String>,
    mut post: Vec<String>,
    finish: &str,
    fx: &ClipFx,
    out: &str,
) {
    let lut = fx.lut.as_ref().and_then(|p| std::fs::read(p).ok()).map(|b| stash("lut", "cube", &b));
    pre.extend(color_chain(fx, lut.as_deref()));
    let mut cur = input;
    let mut step = 0;
    let mut next = |label_base: &str| {
        step += 1;
        format!("[{label_base}_{step}]")
    };

    // ฟิลเตอร์สำเร็จรูป: ถ้าความเข้มไม่เต็ม ผสมกับภาพเดิมด้วย blend
    let preset = preset_filter(fx.filter);
    match preset {
        Some(p) if fx.filter_amount < 0.999 => {
            let (a, b, f, o) = (next(tag), next(tag), next(tag), next(tag));
            chains.push(format!("{cur}{},format=yuv444p,split{a}{b}", pre.join(",")));
            chains.push(format!("{b}{p},format=yuv444p{f}"));
            chains.push(format!("{a}{f}blend=all_mode=normal:all_opacity={:.3}{o}", fx.filter_amount.max(0.0)));
            cur = o;
        }
        Some(p) => {
            pre.push(p.to_owned());
            let o = next(tag);
            chains.push(format!("{cur}{}{o}", pre.join(",")));
            cur = o;
        }
        None => {
            let o = next(tag);
            chains.push(format!("{cur}{}{o}", pre.join(",")));
            cur = o;
        }
    }

    if fx.vignette > 1e-3 {
        post.push(format!("vignette=angle={:.3}", 0.15 + fx.vignette * 0.75));
    }
    match effect_filter(fx.effect, fx.effect_amount) {
        // เอฟเฟกต์ที่แยกสายในตัว (มี ;) ต้องต่อเป็นขั้นแยก
        Some(e) if e.contains(';') => {
            let o = next(tag);
            chains.push(format!("{cur}{}{o}", post.join(",")));
            let e = e.replace("[g", &format!("[{tag}g"));
            let o2 = next(tag);
            chains.push(format!("{o}{e}{o2}"));
            cur = o2;
            post = Vec::new();
        }
        Some(e) => post.push(e),
        None => {}
    }
    post.push(finish.to_owned());
    chains.push(format!("{cur}{}{out}", post.join(",")));
}

/// ภาพซ้อนหนึ่งคลิป จาก `[k:v:0]` ไปเป็น `[out]` (ภาพที่มีพื้นใสเช่น PNG ยังใสอยู่)
/// เวลาของภาพถูกเลื่อนไปตรงตำแหน่งบนไทม์ไลน์ ผู้เรียกเอาไป `overlay` ทับภาพหลักได้เลย
fn overlay_chain(chains: &mut Vec<String>, k: usize, seg: &Seg, spec: VideoSpec, out: &str) {
    let fx = &seg.clip.fx;
    let fps = spec.fps;
    let tag = format!("o{k}");
    let (zw, zh) = zoomed(fx, spec.width, spec.height);
    let mut pre = clip_start(seg, fps);
    pre.extend(orient(fx));
    pre.push(format!("scale={zw}:{zh}:force_original_aspect_ratio=decrease:force_divisible_by=2"));
    // ช่องโปร่งใสแยกไปอีกสาย เพราะฟิลเตอร์ปรับสีหลายตัวทิ้งช่องโปร่งใส แล้วค่อยรวมกลับตอนท้าย
    chains.push(format!("[{k}:v:0]{},format=yuva420p,split[{tag}p][{tag}a]", pre.join(",")));
    let mirror = if fx.effect == Effect::Mirror { ",hflip" } else { "" };
    chains.push(format!("[{tag}a]alphaextract{mirror}[{tag}m]"));
    let rgb = format!("[{tag}c]");
    look_chain(chains, &tag, format!("[{tag}p]"), vec!["setsar=1".to_owned()], vec!["setsar=1".to_owned()], "format=yuv420p", fx, &rgb);

    // เฟด/ความทึบคิดตามเวลาในคลิป (ช่วงอาจเริ่มกลางคลิป) แล้วค่อยเลื่อนไปเวลาบนไทม์ไลน์
    let mut f = vec![format!("{rgb}[{tag}m]alphamerge"), format!("setpts=PTS-STARTPTS+{}/TB", num(seg.clip_offset()))];
    if fx.fade_in > 1e-3 {
        f.push(format!("fade=t=in:st=0:d={}:alpha=1", num(fx.fade_in)));
    }
    if fx.fade_out > 1e-3 {
        f.push(format!("fade=t=out:st={}:d={}:alpha=1", num(seg.clip.dur - fx.fade_out), num(fx.fade_out)));
    }
    let opacity = fx.opacity.clamp(0.0, 1.0);
    if opacity < 0.999 {
        f.push(format!("lut=a='val*{opacity:.3}'"));
    }
    f.push(format!("settb=1/{fps}"));
    f.push(format!("setpts=PTS-STARTPTS+{}/TB{out}", num(seg.offset)));
    chains.push(f.join(","));
}

/// วางภาพซ้อนทับ `base` ตามตำแหน่ง (pan) เฉพาะช่วงเวลาของคลิป
fn overlay_onto(base: &str, ov: &str, seg: &Seg, out: &str) -> String {
    let px = (1.0 + seg.clip.fx.pan_x.clamp(-1.0, 1.0)) / 2.0;
    let py = (1.0 + seg.clip.fx.pan_y.clamp(-1.0, 1.0)) / 2.0;
    format!(
        "{base}{ov}overlay=x='(W-w)*{px:.4}':y='(H-h)*{py:.4}':eof_action=pass:enable='between(t,{},{})'{out}",
        num(seg.offset),
        num(seg.offset + seg.len)
    )
}

fn xfade_name(k: TransitionKind) -> &'static str {
    match k {
        TransitionKind::Fade => "fade",
        TransitionKind::Dissolve => "dissolve",
        TransitionKind::FadeBlack => "fadeblack",
        TransitionKind::FadeWhite => "fadewhite",
        TransitionKind::WipeLeft => "wipeleft",
        TransitionKind::WipeRight => "wiperight",
        TransitionKind::SlideLeft => "slideleft",
        TransitionKind::SlideUp => "slideup",
        TransitionKind::SmoothLeft => "smoothleft",
        TransitionKind::CircleOpen => "circleopen",
        TransitionKind::CircleClose => "circleclose",
        TransitionKind::Radial => "radial",
        TransitionKind::Pixelize => "pixelize",
        TransitionKind::ZoomIn => "zoomin",
        TransitionKind::HBlur => "hblur",
        TransitionKind::WipeUp => "wipeup",
        TransitionKind::WipeDown => "wipedown",
        TransitionKind::SlideRight => "slideright",
        TransitionKind::SlideDown => "slidedown",
        TransitionKind::SmoothRight => "smoothright",
        TransitionKind::SmoothUp => "smoothup",
        TransitionKind::SmoothDown => "smoothdown",
        TransitionKind::CircleCrop => "circlecrop",
        TransitionKind::RectCrop => "rectcrop",
        TransitionKind::HorzOpen => "horzopen",
        TransitionKind::VertOpen => "vertopen",
        TransitionKind::DiagTl => "diagtl",
        TransitionKind::SliceLeft => "hlslice",
        TransitionKind::SqueezeH => "squeezeh",
        TransitionKind::FadeGrays => "fadegrays",
        TransitionKind::Distance => "distance",
        TransitionKind::CoverLeft => "coverleft",
        TransitionKind::RevealLeft => "revealleft",
        TransitionKind::WindLeft => "hlwind",
    }
}

/// สูตรของ FFmpeg ที่สร้างจากสูตรภาพเคลื่อนไหวใน `Motion::pose_at` (ค่าคงที่คำนวณทิ้งไว้เลย)
#[derive(Clone)]
struct Ex {
    text: String,
    constant: Option<f32>,
}

impl Ex {
    fn var(text: String) -> Self {
        Self { text, constant: None }
    }
}

impl From<f32> for Ex {
    fn from(v: f32) -> Self {
        let text = if v < 0.0 { format!("({v:.5})") } else { format!("{v:.5}") };
        Self { text, constant: Some(v) }
    }
}

impl std::ops::Add for Ex {
    type Output = Ex;
    fn add(self, o: Ex) -> Ex {
        match (self.constant, o.constant) {
            (Some(a), Some(b)) => Ex::from(a + b),
            (Some(0.0), _) => o,
            (_, Some(0.0)) => self,
            _ => Ex::var(format!("({}+{})", self.text, o.text)),
        }
    }
}

impl std::ops::Sub for Ex {
    type Output = Ex;
    fn sub(self, o: Ex) -> Ex {
        match (self.constant, o.constant) {
            (Some(a), Some(b)) => Ex::from(a - b),
            (_, Some(0.0)) => self,
            _ => Ex::var(format!("({}-{})", self.text, o.text)),
        }
    }
}

impl std::ops::Mul for Ex {
    type Output = Ex;
    fn mul(self, o: Ex) -> Ex {
        match (self.constant, o.constant) {
            (Some(a), Some(b)) => Ex::from(a * b),
            (Some(1.0), _) => o,
            (_, Some(1.0)) => self,
            _ => Ex::var(format!("{}*{}", self.text, o.text)),
        }
    }
}

impl MotionNum for Ex {
    fn sin(self) -> Self {
        match self.constant {
            Some(v) => Ex::from(v.sin()),
            None => Ex::var(format!("sin({})", self.text)),
        }
    }
    fn clamp01(self) -> Self {
        match self.constant {
            Some(v) => Ex::from(v.clamp(0.0, 1.0)),
            None => Ex::var(format!("clip({},0,1)", self.text)),
        }
    }
}

/// ฟอนต์ที่ผู้ใช้เลือก คัดลอกมาไว้ในโฟลเดอร์ทำงาน (ตั้งชื่อตามที่อยู่ไฟล์เดิม)
fn chosen_font(style: &TextStyle) -> Option<String> {
    if style.font.is_empty() {
        return None;
    }
    let face = crate::fonts::find(&style.font)?.face(style.bold, style.italic);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    face.path.hash(&mut h);
    let ext = face.path.extension().and_then(|e| e.to_str()).unwrap_or("ttf");
    let name = format!("ff{:016x}.{ext}", h.finish());
    let dest = work_dir().join(&name);
    if !dest.exists() {
        std::fs::copy(&face.path, &dest).ok()?;
    }
    Some(name)
}

fn drawtext(style: &TextStyle, spec: VideoSpec, seg: &Seg) -> Option<String> {
    let content = style.content.trim_end();
    if content.is_empty() {
        return None;
    }
    let font = chosen_font(style).or_else(|| font_file(style.bold)).or_else(|| font_file(false));
    let textfile = stash("t", "txt", content.replace("\r\n", "\n").as_bytes());
    let px = (style.size as f64 / 100.0 * spec.height as f64).max(4.0);
    let align = match style.align {
        TextAlign::Center => "C",
        TextAlign::Left => "L",
        TextAlign::Right => "R",
    };
    let mut s = format!(
        "drawtext=textfile={textfile}:expansion=none:text_align={align}:fontcolor={}:line_spacing={:.0}",
        color(style.color),
        px * style.line_spacing as f64 / 100.0
    );
    let opacity = style.opacity.clamp(0.0, 1.0);
    if style.motion.is_static() {
        s.push_str(&format!(":fontsize={px:.1}:x=w*{:.4}-text_w/2:y=h*{:.4}-text_h/2", style.x, style.y));
        if opacity < 0.999 {
            s.push_str(&format!(":alpha={opacity:.3}"));
        }
    } else {
        // เวลานับจากหัวคลิป (t ของ filter นับจากหัวช่วงที่เรนเดอร์)
        let t = Ex::var(format!("(t-{:.6})", seg.offset - seg.clip_offset()));
        let pose = style.motion.pose_at(t, seg.clip.dur as f32);
        s.push_str(&format!(
            ":fontsize='max(1,{px:.1}*{})':x='w*{:.4}-text_w/2+h*{}':y='h*{:.4}-text_h/2+h*{}':alpha='clip({opacity:.3}*{},0,1)'",
            pose.scale.text, style.x, pose.dx.text, style.y, pose.dy.text, pose.alpha.text
        ));
    }
    match font {
        Some(f) => s.push_str(&format!(":fontfile={f}")),
        None => s.push_str(":font=Tahoma"),
    }
    if style.outline > 0.01 {
        s.push_str(&format!(":borderw={:.1}:bordercolor={}", px * style.outline as f64 / 100.0, color(style.outline_color)));
    }
    if style.shadow {
        let (sx, sy) = style.shadow_xy();
        let (sx, sy) = ((px * sx as f64 / 100.0).round(), (px * sy as f64 / 100.0).round());
        s.push_str(&format!(":shadowx={sx:.0}:shadowy={sy:.0}:shadowcolor={}", color(style.shadow_color)));
    }
    if style.background {
        s.push_str(&format!(":box=1:boxcolor={}:boxborderw={:.0}", color(style.background_color), px * style.box_padding as f64 / 100.0));
    }
    s.push_str(&format!(":enable='between(t,{},{})'", num(seg.offset), num(seg.offset + seg.len)));
    Some(s)
}

// ---------------- เสียง ----------------

fn voice_filter(v: Voice, rate: u32) -> Option<String> {
    Some(match v {
        Voice::None => return None,
        Voice::Chipmunk => "rubberband=pitch=1.6".to_owned(),
        Voice::Deep => "rubberband=pitch=0.72".to_owned(),
        Voice::Robot => "afftfilt=real='hypot(re,im)*sin(0)':imag='hypot(re,im)*cos(0)':win_size=512:overlap=0.75".to_owned(),
        Voice::Phone => "highpass=f=320,lowpass=f=3200,acompressor=threshold=0.1:ratio=6".to_owned(),
        Voice::Echo => "aecho=0.8:0.85:350:0.45".to_owned(),
        Voice::Hall => format!("aecho=0.8:0.88:60|120|190:0.4|0.3|0.2,aresample={rate}"),
    })
}

/// ลดเสียงรบกวน + เสียงทุ้ม/แหลม
fn sound_filters(fx: &ClipFx) -> Vec<String> {
    let mut v = Vec::new();
    if fx.denoise > 1e-3 {
        v.push(format!("afftdn=nr={:.1}:tn=1", 6.0 + fx.denoise.min(1.0) * 34.0));
    }
    if fx.bass.abs() > 0.05 {
        v.push(format!("bass=g={:.1}:f=110", fx.bass.clamp(-24.0, 24.0)));
    }
    if fx.treble.abs() > 0.05 {
        v.push(format!("treble=g={:.1}:f=3500", fx.treble.clamp(-24.0, 24.0)));
    }
    v
}

/// เปลี่ยนคีย์ (ครึ่งเสียง) ความยาวเสียงเท่าเดิม
fn pitch_filter(fx: &ClipFx) -> Option<String> {
    (fx.pitch.abs() > 0.05).then(|| format!("rubberband=pitch={:.5}", 2f32.powf(fx.pitch.clamp(-24.0, 24.0) / 12.0)))
}

/// ความดังของคลิป รวมเฟดเข้า/ออก (เฟดยาวอย่างน้อยเท่าทรานซิชัน เพื่อให้เสียงข้ามกันนุ่ม ๆ)
fn gain_expr(seg: &Seg, trans_in: f64, trans_out: f64) -> Option<String> {
    let fx = &seg.clip.fx;
    let fi = fx.fade_in.max(trans_in);
    let fo = fx.fade_out.max(trans_out);
    let vol = fx.volume as f64;
    if fi < 1e-3 && fo < 1e-3 && (vol - 1.0).abs() < 1e-3 {
        return None;
    }
    let off = seg.clip_offset();
    let len = seg.clip.dur;
    let mut e = format!("{vol:.4}");
    if fi > 1e-3 {
        e.push_str(&format!("*min(1,max(0,(t+{})/{}))", num(off), num(fi)));
    }
    if fo > 1e-3 {
        e.push_str(&format!("*min(1,max(0,({}-t-{})/{}))", num(len), num(off), num(fo)));
    }
    Some(format!("volume='{e}':eval=frame"))
}

fn audio_chain(chains: &mut Vec<String>, k: usize, seg: &Seg, rate: u32, trans: (f64, f64)) -> String {
    let label = format!("[a{k}]");
    let delay_ms = (seg.offset * 1000.0).round() as u64;
    let mut f = vec![
        format!("[{k}:a:0]atrim=duration={}", num(seg.len)),
        "asetpts=PTS-STARTPTS".to_owned(),
        format!("aresample={rate}"),
        "aformat=sample_fmts=fltp:channel_layouts=stereo".to_owned(),
    ];
    f.extend(sound_filters(&seg.clip.fx));
    f.extend(voice_filter(seg.clip.fx.voice, rate));
    f.extend(pitch_filter(&seg.clip.fx));
    f.extend(gain_expr(seg, trans.0, trans.1));
    f.push(format!("adelay={delay_ms}:all=1"));
    chains.push(format!("{}{label}", f.join(",")));
    label
}

/// สร้าง graph สำหรับช่วงเวลา [from, until)
/// `video` = None ถ้าไม่ต้องการภาพ, `audio_rate` = None ถ้าไม่ต้องการเสียง
pub fn build(tl: &Timeline, from: f64, until: f64, video: Option<VideoSpec>, audio_rate: Option<u32>) -> Graph {
    let span = (until - from).max(1.0 / 1000.0);
    let mut inputs = Vec::new();
    let mut chains: Vec<String> = Vec::new();
    let mut audio_labels: Vec<String> = Vec::new();
    let mut n_inputs = 0usize;
    let fps = video.map(|v| v.fps).unwrap_or(30);

    // ทรานซิชันต้องเรนเดอร์ครบทั้งช่วง (ไม่งั้นจังหวะผิด) ถ้าหน้าต่างเวลาตัดกลางทรานซิชันให้ขยายออก แล้วตัดทิ้งตอนท้าย
    let r0 = if video.is_some() { transition_span(tl, from).map_or(from, |s| s.0) } else { from };
    let r1 = if video.is_some() { transition_span(tl, until).map_or(until, |s| s.1) } else { until };
    let main_idx = |c: &Clip| tl.main.iter().position(|m| m.id == c.id).unwrap_or(0);
    let trans_of = |c: &Clip| {
        let i = main_idx(c);
        let tin = if i > 0 { tl.transition_after(i - 1).map_or(0.0, |t| t.1) } else { 0.0 };
        let tout = tl.transition_after(i).map_or(0.0, |t| t.1);
        (tin, tout)
    };

    // ---- แทร็กหลัก: ภาพ ----
    let vmain = segments(&tl.main, r0, r1);
    let mut vlabels: Vec<(String, &Seg)> = Vec::new();
    if let Some(spec) = video {
        for (i, seg) in vmain.iter().enumerate() {
            push_input(&mut inputs, seg, fps);
            let k = n_inputs;
            n_inputs += 1;
            let out = format!("[v{i}]");
            video_chain(&mut chains, k, seg, spec, &out);
            vlabels.push((out, seg));
        }
    }

    // ---- ภาพซ้อน ----
    let overlays: &[Clip] = if tl.overlay_hidden { &[] } else { &tl.overlay };
    let vover = segments(overlays, r0, r1);
    let mut olabels: Vec<(String, &Seg)> = Vec::new();
    if let Some(spec) = video {
        for (i, seg) in vover.iter().enumerate() {
            push_input(&mut inputs, seg, fps);
            let k = n_inputs;
            n_inputs += 1;
            let out = format!("[ov{i}]");
            overlay_chain(&mut chains, k, seg, spec, &out);
            olabels.push((out, seg));
        }
    }

    // ---- แทร็กหลัก + ภาพซ้อน: เสียง (ใช้ช่วง [from, until) ตรง ๆ) ----
    if let Some(rate) = audio_rate {
        let main = if tl.main_muted { Vec::new() } else { segments(&tl.main, from, until) };
        for (seg, trans) in
            main.iter().map(|s| (s, trans_of(s.clip))).chain(segments(overlays, from, until).iter().map(|s| (s, (0.0, 0.0))))
        {
            if !seg.clip.has_audio {
                continue;
            }
            push_input(&mut inputs, seg, fps);
            let k = n_inputs;
            n_inputs += 1;
            audio_labels.push(audio_chain(&mut chains, k, seg, rate, trans));
        }
    }

    if let Some(v) = video {
        // ต่อคลิปเข้าด้วยกัน: ทับกัน = xfade, ไม่ทับ = concat
        let mut acc: Option<(String, f64)> = None; // (ป้าย, เวลาจบของสายที่ต่อแล้ว)
        for (i, (label, seg)) in vlabels.iter().enumerate() {
            let end = seg.offset + seg.len;
            acc = Some(match acc {
                None => (label.clone(), end),
                Some((prev, prev_end)) => {
                    let out = format!("[j{i}]");
                    let overlap = prev_end - seg.offset;
                    if overlap > 1e-3 {
                        let kind = tl.transition_after(main_idx(seg.clip).saturating_sub(1)).map_or(TransitionKind::Fade, |t| t.0);
                        chains.push(format!(
                            "{prev}{label}xfade=transition={}:duration={}:offset={}{out}",
                            xfade_name(kind),
                            num(overlap),
                            num(seg.offset)
                        ));
                    } else {
                        chains.push(format!("{prev}{label}concat=n=2:v=1:a=0{out}"));
                    }
                    (out, end)
                }
            });
        }
        // ช่วงท้ายที่ไม่มีภาพ (เช่นเพลงยาวกว่าวิดีโอ) เป็นจอดำ
        let render_span = (r1 - r0).max(1.0 / 1000.0);
        let main_end = acc.as_ref().map_or(0.0, |a| a.1);
        let gap = render_span - main_end;
        let gap_label = "[vgap]";
        let mut base = match acc {
            Some((l, _)) if gap <= 1e-3 => l,
            other => {
                chains.push(format!(
                    "color=c=black:s={}x{}:r={}:d={},setsar=1,format=yuv420p,settb=1/{}{gap_label}",
                    v.width,
                    v.height,
                    v.fps,
                    num(gap.max(1.0 / v.fps as f64)),
                    v.fps
                ));
                match other {
                    Some((l, _)) => {
                        chains.push(format!("{l}{gap_label}concat=n=2:v=1:a=0[vfull]"));
                        "[vfull]".to_owned()
                    }
                    None => gap_label.to_owned(),
                }
            }
        };

        for (i, (label, seg)) in olabels.iter().enumerate() {
            let out = format!("[vov{i}]");
            chains.push(overlay_onto(&base, label, seg, &out));
            base = out;
        }

        // ---- ข้อความ + ซับไตเติล ----
        let mut texts = Vec::new();
        for (kind, hidden) in [(TrackKind::Text, tl.text_hidden), (TrackKind::Subtitle, tl.subtitles_hidden)] {
            if hidden {
                continue;
            }
            for seg in segments(tl.track(kind), r0, r1) {
                if let Some(style) = &seg.clip.text {
                    texts.extend(drawtext(style, v, &seg));
                }
            }
        }
        if !texts.is_empty() {
            chains.push(format!("{base}{}[vtext]", texts.join(",")));
            base = "[vtext]".to_owned();
        }

        // ตัดส่วนที่ขยายเผื่อทรานซิชันทิ้ง
        chains.push(format!("{base}trim=start={}:duration={},setpts=PTS-STARTPTS[vout]", num(from - r0), num(span)));
    }

    // ---- แทร็กเสียง + มิกซ์ ----
    if let Some(rate) = audio_rate {
        if !tl.audio_muted {
            for seg in segments(&tl.audio, from, until) {
                push_input(&mut inputs, &seg, fps);
                let k = n_inputs;
                n_inputs += 1;
                audio_labels.push(audio_chain(&mut chains, k, &seg, rate, (0.0, 0.0)));
            }
        }
        // เสียงเงียบยาวเท่าช่วงเวลา เป็นฐานให้ความยาวเสียงตรงกับภาพเสมอ
        chains.push(format!("anullsrc=r={rate}:cl=stereo,atrim=duration={}[abase]", num(span)));
        if audio_labels.is_empty() {
            chains.push("[abase]anull[aout]".to_owned());
        } else {
            chains.push(format!(
                "[abase]{}amix=inputs={}:duration=first:dropout_transition=0:normalize=0[aout]",
                audio_labels.concat(),
                audio_labels.len() + 1
            ));
        }
    }

    Graph { inputs, filter: chains.join(";\n"), cwd: work_dir() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fx::Transition;
    use crate::core::media::MediaInfo;

    fn info(dur: f64, audio: bool) -> MediaInfo {
        MediaInfo { duration: Some(dur), width: 1920, height: 1080, fps: Some(30.0), has_video: true, has_audio: audio }
    }

    fn sample() -> Timeline {
        let mut tl = Timeline::default();
        let a = tl.new_clip("a.mp4".into(), "a".into(), MediaKind::Video, &info(10.0, true));
        let b = tl.new_clip("b.mp4".into(), "b".into(), MediaKind::Video, &info(10.0, false));
        tl.insert_main(0, a);
        tl.insert_main(1, b);
        let m = tl.new_clip("m.mp3".into(), "m".into(), MediaKind::Audio, &info(30.0, true));
        tl.place_audio(m, 0.0);
        tl
    }

    #[test]
    fn window_cuts_clips() {
        let tl = sample();
        let spec = VideoSpec { width: 640, height: 360, fps: 30 };
        let g = build(&tl, 5.0, 30.0, Some(spec), Some(48000));
        let args: Vec<String> = g.inputs.iter().map(|s| s.to_string_lossy().into_owned()).collect();
        // คลิปแรกเริ่มอ่านที่วินาทีที่ 5 ยาว 5 วิ
        assert_eq!(&args[0..4], ["-ss", "5.000000", "-t", "5.000000"]);
        assert!(g.filter.contains("[vgap]"), "เพลงยาวกว่าวิดีโอต้องมีจอดำต่อท้าย");
        assert!(g.filter.contains("amix=inputs=3"), "{}", g.filter);
        assert!(g.filter.contains("trim=start=0.000000:duration=25.000000"));

        assert_eq!(preview_until(&tl, 0.0, 2), 10.0);
        assert_eq!(preview_until(&tl, 0.0, 16), 30.0);
    }

    #[test]
    fn transition_window_expands() {
        let mut tl = sample();
        let id = tl.main[0].id;
        tl.update_clip(id, |c| c.fx.transition = Some(Transition { kind: TransitionKind::Dissolve, dur: 1.0 }));
        // คลิปที่สองเริ่มที่ 9 ทรานซิชัน 9..10 ขอภาพตั้งแต่ 9.5 ต้องเรนเดอร์ตั้งแต่ 9 แล้วตัดทิ้ง 0.5
        let spec = VideoSpec { width: 640, height: 360, fps: 30 };
        let g = build(&tl, 9.5, 12.0, Some(spec), None);
        assert!(g.filter.contains("xfade=transition=dissolve:duration=1.000000:offset=0.000000"), "{}", g.filter);
        assert!(g.filter.contains("trim=start=0.500000:duration=2.500000"), "{}", g.filter);
        // เสียงคลิปแรกต้องเฟดออกตอนทรานซิชัน
        let g = build(&tl, 0.0, 12.0, None, Some(48000));
        assert!(g.filter.contains("volume='1.0000*min(1,max(0,(10.000000-t-0.000000)/1.000000))'"), "{}", g.filter);
    }

    #[test]
    fn overlay_composites_on_main() {
        let mut tl = sample();
        let mut pic = tl.new_clip("logo.png".into(), "logo".into(), MediaKind::Image, &info(0.0, false));
        pic.fx = ClipFx { fade_in: 1.0, opacity: 0.5, pan_x: 1.0, ..ClipFx::default() };
        tl.place_overlay(pic, 2.0);
        let pip = tl.new_clip("pip.mp4".into(), "pip".into(), MediaKind::Video, &info(10.0, true));
        tl.place_overlay(pip, 8.0);
        let spec = VideoSpec { width: 640, height: 360, fps: 30 };

        // ขอภาพตั้งแต่ 3 วิ: รูปเริ่มกลางคลิป (เฟดคิดจากหัวคลิป) แล้วเลื่อนไปวางที่ 0
        let g = build(&tl, 3.0, 12.0, Some(spec), Some(48000));
        let f = &g.filter;
        assert!(f.contains("scale=320:180:force_original_aspect_ratio=decrease"), "{f}");
        assert!(f.contains("alphamerge,setpts=PTS-STARTPTS+1.000000/TB,fade=t=in:st=0:d=1.000000:alpha=1,lut=a='val*0.500'"), "{f}");
        assert!(f.contains("[ov0]overlay=x='(W-w)*1.0000':y='(H-h)*0.5000':eof_action=pass:enable='between(t,0.000000,4.000000)'"), "{f}");
        assert!(f.contains("setpts=PTS-STARTPTS+5.000000/TB[ov1]"), "{f}");
        // ภาพซ้อนอยู่ใต้ข้อความ ตัดท้ายด้วย [vout] และเสียงของวิดีโอซ้อนถูกมิกซ์ด้วย
        assert!(f.contains("[vov1]trim=start="), "{f}");
        assert!(f.contains("amix=inputs=4"), "{f}");

        tl.overlay_hidden = true;
        let g = build(&tl, 3.0, 12.0, Some(spec), Some(48000));
        assert!(!g.filter.contains("overlay=") && g.filter.contains("amix=inputs=3"), "{}", g.filter);
    }

    #[test]
    fn default_layout_is_plain_fit() {
        let fx = ClipFx::default();
        assert_eq!(layout_pre(&fx, 1920, 1080), ["scale=1920:1080:force_original_aspect_ratio=decrease:force_divisible_by=2"]);
        assert_eq!(layout_pad(&fx, 1920, 1080), "pad=1920:1080:x='(ow-iw)*0.5000':y='(oh-ih)*0.5000':color=0x000000");
        assert_eq!(effect_filter(Effect::Blur, 1.0).unwrap(), "gblur=sigma=8.00");
        assert!(effect_filter(Effect::Blur, 0.0).is_none());
        assert!(sound_filters(&fx).is_empty() && pitch_filter(&fx).is_none() && color_chain(&fx, None).is_empty());
    }

    #[test]
    fn cover_zoom_crops_with_pan() {
        let fx = ClipFx { fit: Fit::Cover, zoom: 2.0, pan_x: 1.0, rotate: 1, ..ClipFx::default() };
        let v = layout_pre(&fx, 640, 360);
        assert_eq!(v[0], "transpose=clock");
        assert_eq!(v[1], "scale=1280:720:force_original_aspect_ratio=increase:force_divisible_by=2");
        // เลื่อนไปขวาสุด = เห็นด้านซ้ายของภาพ
        assert_eq!(v[2], "crop=w='min(iw,640)':h='min(ih,360)':x='(iw-ow)*0.0000':y='(ih-oh)*0.5000'");
    }

    #[test]
    fn sound_and_color_extras() {
        let fx = ClipFx { pitch: 12.0, bass: 6.0, denoise: 0.5, hue: 0.5, fade: 1.0, ..ClipFx::default() };
        assert_eq!(pitch_filter(&fx).unwrap(), "rubberband=pitch=2.00000");
        assert_eq!(sound_filters(&fx), ["afftdn=nr=23.0:tn=1", "bass=g=6.0:f=110"]);
        let c = color_chain(&fx, None).join(",");
        assert!(c.contains("hue=h=90.0") && c.contains("curves=all='0/0.250 0.25/0.270"), "{c}");
    }
}
