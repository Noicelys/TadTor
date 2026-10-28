//! ไทม์ไลน์: แทร็กหลัก (ภาพ/วิดีโอ ต่อกันอัตโนมัติไม่มีช่องว่าง แบบ CapCut)
//! + แทร็กภาพซ้อน (ซ้อนทับกันได้หลายชั้น) / ข้อความ / ซับไตเติล / เสียง
//!
//! ทุกฟังก์ชันที่แก้ไทม์ไลน์อยู่ที่นี่ หน้าจอแค่เรียกใช้ ทำให้ Undo/Redo ทำได้ง่าย ๆ ด้วยการเก็บสำเนา

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::fx::{ClipFx, TextStyle, TransitionKind};
use super::media::{MediaInfo, MediaKind};

/// คลิปสั้นสุดที่ยอมให้ตัดเหลือ (วินาที)
pub const MIN_CLIP: f64 = 0.1;
/// รูปภาพที่ลากลงไทม์ไลน์ยาวเท่านี้ก่อน (ยืดหดได้ทีหลัง)
pub const IMAGE_DEFAULT: f64 = 5.0;
/// ข้อความใหม่ยาวเท่านี้
pub const TEXT_DEFAULT: f64 = 3.0;
/// ขนาดเริ่มต้นของภาพซ้อน (สัดส่วนของจอ)
pub const OVERLAY_ZOOM: f32 = 0.5;
const EPS: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    /// ภาพ/วิดีโอที่วางซ้อนบนแทร็กหลัก
    Overlay,
    Main,
    Text,
    Subtitle,
    Audio,
}

impl TrackKind {
    /// ลำดับแทร็กบนจอ (บนลงล่าง)
    pub const ALL: [TrackKind; 5] = [TrackKind::Overlay, TrackKind::Main, TrackKind::Text, TrackKind::Subtitle, TrackKind::Audio];

    pub fn label(self) -> &'static str {
        match self {
            TrackKind::Overlay => "ภาพซ้อน",
            TrackKind::Main => "แทร็กหลัก",
            TrackKind::Text => "ข้อความ",
            TrackKind::Subtitle => "ซับไตเติล",
            TrackKind::Audio => "เสียง",
        }
    }

    pub fn is_text(self) -> bool {
        matches!(self, TrackKind::Text | TrackKind::Subtitle)
    }

    /// แทร็กที่มีภาพจากไฟล์ (แทร็กหลัก / ภาพซ้อน)
    pub fn is_visual(self) -> bool {
        matches!(self, TrackKind::Main | TrackKind::Overlay)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: u64,
    /// ไฟล์ต้นฉบับ (คลิปข้อความไม่มีไฟล์ = ว่าง)
    pub path: PathBuf,
    pub name: String,
    pub kind: MediaKind,
    /// ตำแหน่งบนไทม์ไลน์ (แทร็กหลักคำนวณใหม่เสมอด้วย `pack`)
    pub start: f64,
    /// จุดเริ่มในไฟล์ต้นฉบับ
    pub src_in: f64,
    /// ความยาวบนไทม์ไลน์
    pub dur: f64,
    /// ความยาวไฟล์ต้นฉบับ (None = รูปภาพ/ข้อความ ยืดได้ไม่จำกัด)
    pub src_len: Option<f64>,
    pub has_audio: bool,
    #[serde(default)]
    pub text: Option<TextStyle>,
    #[serde(default)]
    pub fx: ClipFx,
}

impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.dur
    }

    pub fn contains(&self, t: f64) -> bool {
        t > self.start + EPS && t < self.end() - EPS
    }

    /// ชื่อที่แสดงบนไทม์ไลน์
    pub fn title(&self) -> &str {
        match &self.text {
            Some(t) => t.content.lines().next().unwrap_or(""),
            None => &self.name,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub main: Vec<Clip>,
    pub audio: Vec<Clip>,
    #[serde(default)]
    pub overlay: Vec<Clip>,
    #[serde(default)]
    pub text: Vec<Clip>,
    #[serde(default)]
    pub subtitles: Vec<Clip>,
    #[serde(default)]
    pub main_muted: bool,
    #[serde(default)]
    pub audio_muted: bool,
    #[serde(default)]
    pub overlay_hidden: bool,
    #[serde(default)]
    pub text_hidden: bool,
    #[serde(default)]
    pub subtitles_hidden: bool,
    next_id: u64,
}

impl Timeline {
    /// สร้างคลิปใหม่จากไฟล์ในคลังสื่อ (ยังไม่ได้ใส่ลงแทร็ก)
    pub fn new_clip(&mut self, path: PathBuf, name: String, kind: MediaKind, info: &MediaInfo) -> Clip {
        let src_len = match kind {
            MediaKind::Image | MediaKind::Text => None,
            _ => Some(info.duration.unwrap_or(IMAGE_DEFAULT).max(MIN_CLIP)),
        };
        Clip {
            id: self.fresh_id(),
            path,
            name,
            kind,
            start: 0.0,
            src_in: 0.0,
            dur: src_len.unwrap_or(IMAGE_DEFAULT),
            src_len,
            has_audio: info.has_audio,
            text: None,
            fx: ClipFx::default(),
        }
    }

    pub fn new_text_clip(&mut self, style: TextStyle, dur: f64) -> Clip {
        Clip {
            id: self.fresh_id(),
            path: PathBuf::new(),
            name: String::new(),
            kind: MediaKind::Text,
            start: 0.0,
            src_in: 0.0,
            dur: dur.max(MIN_CLIP),
            src_len: None,
            has_audio: false,
            text: Some(style),
            fx: ClipFx::default(),
        }
    }

    fn fresh_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// ความยาวทรานซิชันจริงระหว่างคลิปหลักลำดับ i กับ i+1 (จำกัดไม่ให้เกินครึ่งของคลิปที่สั้นกว่า)
    pub fn transition_after(&self, i: usize) -> Option<(TransitionKind, f64)> {
        let (a, b) = (self.main.get(i)?, self.main.get(i + 1)?);
        let t = a.fx.transition?;
        let d = t.dur.min(a.dur / 2.0).min(b.dur / 2.0);
        (d >= 0.05).then_some((t.kind, d))
    }

    /// ต่อคลิปในแทร็กหลักให้ชิดกัน (ช่วงทรานซิชันให้คลิปซ้อนกัน)
    pub fn pack(&mut self) {
        let overlaps: Vec<f64> = (0..self.main.len()).map(|i| self.transition_after(i).map_or(0.0, |t| t.1)).collect();
        let mut t = 0.0;
        for (c, o) in self.main.iter_mut().zip(overlaps) {
            c.start = t;
            t += c.dur - o;
        }
    }

    pub fn main_end(&self) -> f64 {
        self.main.last().map(Clip::end).unwrap_or(0.0)
    }

    /// ความยาวงานทั้งหมด
    pub fn duration(&self) -> f64 {
        self.audio.iter().chain(&self.overlay).chain(&self.text).chain(&self.subtitles).map(Clip::end).fold(self.main_end(), f64::max)
    }

    pub fn is_empty(&self) -> bool {
        TrackKind::ALL.iter().all(|k| self.track(*k).is_empty())
    }

    pub fn track(&self, kind: TrackKind) -> &Vec<Clip> {
        match kind {
            TrackKind::Overlay => &self.overlay,
            TrackKind::Main => &self.main,
            TrackKind::Text => &self.text,
            TrackKind::Subtitle => &self.subtitles,
            TrackKind::Audio => &self.audio,
        }
    }

    pub fn track_mut(&mut self, kind: TrackKind) -> &mut Vec<Clip> {
        match kind {
            TrackKind::Overlay => &mut self.overlay,
            TrackKind::Main => &mut self.main,
            TrackKind::Text => &mut self.text,
            TrackKind::Subtitle => &mut self.subtitles,
            TrackKind::Audio => &mut self.audio,
        }
    }

    pub fn all_clips(&self) -> impl Iterator<Item = &Clip> {
        TrackKind::ALL.into_iter().flat_map(move |k| self.track(k).iter())
    }

    pub fn find(&self, id: u64) -> Option<(TrackKind, usize)> {
        TrackKind::ALL.into_iter().find_map(|k| self.track(k).iter().position(|c| c.id == id).map(|i| (k, i)))
    }

    pub fn clip(&self, id: u64) -> Option<&Clip> {
        let (k, i) = self.find(id)?;
        self.track(k).get(i)
    }

    pub fn clip_mut(&mut self, id: u64) -> Option<&mut Clip> {
        let (k, i) = self.find(id)?;
        self.track_mut(k).get_mut(i)
    }

    /// แก้ค่าของคลิป แล้วจัดตำแหน่งแทร็กหลักใหม่ (เผื่อทรานซิชันเปลี่ยน)
    pub fn update_clip(&mut self, id: u64, f: impl FnOnce(&mut Clip)) {
        if let Some(c) = self.clip_mut(id) {
            f(c);
            self.pack();
        }
    }

    /// คลิปในแทร็กที่อยู่ใต้เวลา t (ถ้าซ้อนกันหลายชั้น คืนชั้นบนสุด)
    pub fn clip_at(&self, kind: TrackKind, t: f64) -> Option<&Clip> {
        self.track(kind).iter().rev().find(|c| t >= c.start && t < c.end())
    }

    /// จัดแถวย่อยของแทร็กภาพซ้อน คืน `(จำนวนแถวอย่างน้อย 1, แถวของแต่ละคลิปตามลำดับใน self.overlay)`
    /// คลิปที่เวลาไม่ทับกันจะอยู่แถวเดียวกัน คลิปที่เวลาทับกันจะขึ้นแถวใหม่ด้านบน
    pub fn overlay_rows(&self) -> (usize, Vec<usize>) {
        let mut row_ends: Vec<f64> = Vec::new();
        let mut rows = Vec::with_capacity(self.overlay.len());
        for c in &self.overlay {
            if let Some(r) = row_ends.iter().position(|&end| c.start >= end - EPS) {
                row_ends[r] = c.end();
                rows.push(r);
            } else {
                rows.push(row_ends.len());
                row_ends.push(c.end());
            }
        }
        (row_ends.len().max(1), rows)
    }

    // ---------- เพิ่ม / ย้าย / ลบ ----------

    pub fn insert_main(&mut self, index: usize, clip: Clip) -> u64 {
        let id = clip.id;
        let index = index.min(self.main.len());
        self.main.insert(index, clip);
        self.pack();
        id
    }

    /// วางคลิปในแทร็กอิสระ (ภาพซ้อนวางทับกันได้ แทร็กอื่นวางใกล้ `at` ที่สุดโดยไม่ทับคลิปอื่น)
    pub fn place(&mut self, kind: TrackKind, mut clip: Clip, at: f64) -> u64 {
        debug_assert!(kind != TrackKind::Main);
        clip.start = if kind == TrackKind::Overlay {
            at.max(0.0)
        } else {
            self.free_start(kind, None, at, clip.dur)
        };
        let id = clip.id;
        self.track_mut(kind).push(clip);
        self.sort_track(kind);
        id
    }

    pub fn place_audio(&mut self, clip: Clip, at: f64) -> u64 {
        self.place(TrackKind::Audio, clip, at)
    }

    /// วางภาพ/วิดีโอซ้อนบนแทร็กหลัก (ยังไม่เคยปรับขนาด = ย่อเหลือ `OVERLAY_ZOOM`)
    pub fn place_overlay(&mut self, mut clip: Clip, at: f64) -> u64 {
        if (clip.fx.zoom - 1.0).abs() < 1e-3 {
            clip.fx.zoom = OVERLAY_ZOOM;
        }
        self.place(TrackKind::Overlay, clip, at)
    }

    /// ย้ายคลิประหว่างแทร็กหลักกับแทร็กภาพซ้อน (`at` = เวลาที่วาง แทร็กหลักแทรกที่รอยต่อใกล้สุด)
    /// ขนาด/ตำแหน่งคืนค่าเริ่มต้นของแทร็กใหม่ เพราะสองแทร็กตีความค่าต่างกัน
    pub fn move_to_track(&mut self, id: u64, to: TrackKind, at: f64) -> bool {
        let Some((from, i)) = self.find(id) else {
            return false;
        };
        if from == to || !from.is_visual() || !to.is_visual() {
            return false;
        }
        let mut c = self.track_mut(from).remove(i);
        c.fx.transition = None;
        c.fx.zoom = 1.0;
        c.fx.pan_x = 0.0;
        c.fx.pan_y = 0.0;
        self.pack();
        if to == TrackKind::Main {
            let index = self.main.iter().filter(|m| (m.start + m.end()) / 2.0 < at).count();
            self.insert_main(index, c);
        } else {
            self.place_overlay(c, at);
        }
        true
    }

    fn sort_track(&mut self, kind: TrackKind) {
        self.track_mut(kind).sort_by(|a, b| a.start.total_cmp(&b.start));
    }

    /// หาเวลาเริ่มที่ใกล้ `desired` ที่สุดซึ่งวางคลิปยาว `dur` ได้โดยไม่ทับใคร
    pub fn free_start(&self, kind: TrackKind, exclude: Option<u64>, desired: f64, dur: f64) -> f64 {
        let desired = desired.max(0.0);
        let others: Vec<&Clip> = self.track(kind).iter().filter(|c| Some(c.id) != exclude).collect();
        let fits = |s: f64| s >= -EPS && others.iter().all(|c| s + dur <= c.start + EPS || s >= c.end() - EPS);
        let mut candidates = vec![desired];
        for c in &others {
            candidates.push(c.end());
            candidates.push(c.start - dur);
        }
        candidates
            .into_iter()
            .filter(|s| fits(*s))
            .min_by(|a, b| (a - desired).abs().total_cmp(&(b - desired).abs()))
            .unwrap_or_else(|| others.iter().map(|c| c.end()).fold(0.0, f64::max))
            .max(0.0)
    }

    /// ย้ายคลิปในแทร็กหลักไปไว้ลำดับที่ `to` (นับตามรายการก่อนย้าย)
    pub fn move_main(&mut self, id: u64, to: usize) {
        let Some(from) = self.main.iter().position(|c| c.id == id) else {
            return;
        };
        let clip = self.main.remove(from);
        let to = if to > from { to - 1 } else { to };
        self.main.insert(to.min(self.main.len()), clip);
        self.pack();
    }

    /// ย้ายคลิปในแทร็กอิสระ
    pub fn move_free(&mut self, id: u64, desired: f64) {
        let Some((kind, i)) = self.find(id).filter(|(k, _)| *k != TrackKind::Main) else {
            return;
        };
        let dur = self.track(kind)[i].dur;
        let start = if kind == TrackKind::Overlay {
            desired.max(0.0)
        } else {
            self.free_start(kind, Some(id), desired, dur)
        };
        self.track_mut(kind)[i].start = start;
        self.sort_track(kind);
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let Some((kind, i)) = self.find(id) else {
            return false;
        };
        self.track_mut(kind).remove(i);
        if kind == TrackKind::Main {
            self.pack();
        }
        true
    }

    // ---------- ตัด ----------

    /// แยกคลิปที่เวลา t คืน id ของคลิปครึ่งหลัง
    pub fn split(&mut self, id: u64, t: f64) -> Option<u64> {
        let (kind, i) = self.find(id)?;
        let clip = self.track(kind)[i].clone();
        let left = t - clip.start;
        if left < MIN_CLIP - EPS || clip.dur - left < MIN_CLIP - EPS {
            return None;
        }
        let new_id = self.fresh_id();
        let mut right = clip.clone();
        right.id = new_id;
        right.start = t;
        right.dur = clip.dur - left;
        right.fx.fade_in = 0.0;
        if right.src_len.is_some() {
            right.src_in = clip.src_in + left;
        }
        let track = self.track_mut(kind);
        // ครึ่งแรกเก็บเฟดเข้า ครึ่งหลังเก็บเฟดออกและทรานซิชันไปคลิปถัดไป
        track[i].dur = left;
        track[i].fx.fade_out = 0.0;
        track[i].fx.transition = None;
        track.insert(i + 1, right);
        if kind == TrackKind::Main {
            self.pack();
        }
        Some(new_id)
    }

    fn neighbors(&self, kind: TrackKind, i: usize) -> (f64, Option<f64>) {
        if matches!(kind, TrackKind::Main | TrackKind::Overlay) {
            return (0.0, None);
        }
        let track = self.track(kind);
        let prev_end = if i > 0 { track[i - 1].end() } else { 0.0 };
        (prev_end, track.get(i + 1).map(|c| c.start))
    }

    /// ตัดหัวคลิปออก `delta` วินาที (ติดลบ = ดึงหัวกลับมา) คืนค่าที่ทำได้จริงหลังจำกัดขอบเขต
    pub fn trim_head(&mut self, id: u64, delta: f64) -> f64 {
        let Some((kind, i)) = self.find(id) else {
            return 0.0;
        };
        let (prev_end, _) = self.neighbors(kind, i);
        let c = &mut self.track_mut(kind)[i];
        let mut d = delta.min(c.dur - MIN_CLIP);
        if c.src_len.is_some() {
            d = d.max(-c.src_in);
        }
        if kind != TrackKind::Main {
            d = d.max(prev_end - c.start);
        }
        if c.src_len.is_some() {
            c.src_in += d;
        }
        c.dur -= d;
        if kind != TrackKind::Main {
            c.start += d;
            self.sort_track(kind);
        } else {
            self.pack();
        }
        d
    }

    /// ยืด/หดท้ายคลิป `delta` วินาที (ติดลบ = ตัดท้ายออก)
    pub fn trim_tail(&mut self, id: u64, delta: f64) -> f64 {
        let Some((kind, i)) = self.find(id) else {
            return 0.0;
        };
        let (_, next_start) = self.neighbors(kind, i);
        let c = &mut self.track_mut(kind)[i];
        let mut d = delta.max(MIN_CLIP - c.dur);
        if let Some(len) = c.src_len {
            d = d.min(len - c.src_in - c.dur);
        }
        if let Some(ns) = next_start {
            d = d.min(ns - c.end());
        }
        c.dur += d;
        if kind == TrackKind::Main {
            self.pack();
        }
        d
    }

    /// ตัดช่วงเวลา [from, to) ออกจากงานทั้งหมด (แบบ ripple: ทุกแทร็กที่อยู่หลังช่วงเลื่อนมาชิดกัน)
    /// คลิปที่คร่อมช่วงถูกตัดเฉพาะส่วนที่อยู่ในช่วง คืน false ถ้าช่วงสั้นเกินไป
    pub fn delete_range(&mut self, from: f64, to: f64) -> bool {
        let from = from.max(0.0);
        let len = to - from;
        if len < 0.01 {
            return false;
        }
        for kind in TrackKind::ALL {
            // คลิปที่คร่อมทั้งช่วง: แยกเป็นสองชิ้นที่จุดเริ่มช่วงก่อน
            let spanning: Vec<usize> =
                self.track(kind).iter().enumerate().filter(|(_, c)| c.start < from - EPS && c.end() > to + EPS).map(|(i, _)| i).collect();
            for i in spanning.into_iter().rev() {
                let id = self.fresh_id();
                let track = self.track_mut(kind);
                let left = &mut track[i];
                let cut = from - left.start;
                let mut right = left.clone();
                left.dur = cut;
                left.fx.fade_out = 0.0;
                left.fx.transition = None;
                right.id = id;
                right.start = from;
                right.dur -= cut;
                right.fx.fade_in = 0.0;
                if right.src_len.is_some() {
                    right.src_in += cut;
                }
                track.insert(i + 1, right);
            }
            self.track_mut(kind).retain_mut(|c| {
                let (s, e) = (c.start, c.end());
                if e <= from + EPS {
                    return true;
                }
                if s >= to - EPS {
                    c.start -= len;
                    return true;
                }
                if s >= from - EPS && e <= to + EPS {
                    return false;
                }
                if s < from {
                    // ท้ายคลิปล้ำเข้าไปในช่วง
                    c.dur = from - s;
                    c.fx.fade_out = c.fx.fade_out.min(c.dur / 2.0);
                    c.fx.transition = None;
                } else {
                    // หัวคลิปอยู่ในช่วง
                    let d = to - s;
                    if c.src_len.is_some() {
                        c.src_in += d;
                    }
                    c.dur -= d;
                    c.start = from;
                    c.fx.fade_in = c.fx.fade_in.min(c.dur / 2.0);
                }
                c.dur >= 0.04
            });
        }
        self.pack();
        true
    }

    /// ตั้งความยาวคลิป (ใช้กับรูปภาพ/ข้อความในแผงขวา)
    pub fn set_duration(&mut self, id: u64, dur: f64) {
        if let Some(c) = self.clip(id) {
            let delta = dur - c.dur;
            self.trim_tail(id, delta);
        }
    }

    /// แทนที่ซับไตเติลทั้งหมด (ใช้ตอนสร้างซับอัตโนมัติ / นำเข้า .srt)
    pub fn replace_subtitles(&mut self, items: Vec<(f64, f64, String)>) {
        self.subtitles.clear();
        self.add_subtitles(items, &TextStyle::subtitle(String::new()));
    }

    /// แทนที่ซับเฉพาะช่วง [from, until) (ซับที่คาบเกี่ยวช่วงนี้ถูกลบ ซับนอกช่วงอยู่เหมือนเดิม)
    pub fn replace_subtitles_in(&mut self, from: f64, until: f64, items: Vec<(f64, f64, String)>, style: &TextStyle) {
        self.subtitles.retain(|c| c.end() <= from + EPS || c.start >= until - EPS);
        self.add_subtitles(items, style);
    }

    /// ใส่ซับใหม่ทุกบรรทัดด้วยสไตล์เดียวกัน (ข้ามบรรทัดว่าง/สั้นเกิน)
    fn add_subtitles(&mut self, items: Vec<(f64, f64, String)>, style: &TextStyle) {
        for (s, e, text) in items {
            if e - s < MIN_CLIP / 2.0 || text.trim().is_empty() {
                continue;
            }
            let c = self.new_text_clip(TextStyle { content: text, ..style.clone() }, e - s);
            self.place(TrackKind::Subtitle, c, s);
        }
    }

    /// เลื่อนเวลาซับทั้งหมด (ซับแรกไม่ถอยเกินวินาทีที่ 0)
    pub fn shift_subtitles(&mut self, dt: f64) {
        let first = self.subtitles.iter().map(|c| c.start).fold(f64::INFINITY, f64::min);
        let dt = if first.is_finite() { dt.max(-first) } else { dt };
        for c in &mut self.subtitles {
            c.start += dt;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fx::Transition;

    fn info(dur: f64) -> MediaInfo {
        MediaInfo { duration: Some(dur), width: 1920, height: 1080, fps: Some(30.0), has_video: true, has_audio: true }
    }

    fn video(tl: &mut Timeline, dur: f64) -> Clip {
        tl.new_clip("a.mp4".into(), "a".into(), MediaKind::Video, &info(dur))
    }

    fn audio(tl: &mut Timeline, dur: f64) -> Clip {
        tl.new_clip("a.mp3".into(), "a".into(), MediaKind::Audio, &info(dur))
    }

    #[test]
    fn main_track_packs() {
        let mut tl = Timeline::default();
        let a = video(&mut tl, 4.0);
        let b = video(&mut tl, 6.0);
        let ida = tl.insert_main(0, a);
        let idb = tl.insert_main(0, b);
        assert_eq!(tl.main[0].id, idb);
        assert_eq!(tl.clip(ida).unwrap().start, 6.0);
        assert_eq!(tl.duration(), 10.0);
        tl.move_main(idb, 2);
        assert_eq!(tl.main[0].id, ida);
        assert_eq!(tl.clip(idb).unwrap().start, 4.0);
        tl.remove(ida);
        assert_eq!(tl.clip(idb).unwrap().start, 0.0);
    }

    #[test]
    fn split_and_trim() {
        let mut tl = Timeline::default();
        let a = video(&mut tl, 10.0);
        let id = tl.insert_main(0, a);
        let right = tl.split(id, 4.0).unwrap();
        assert_eq!(tl.clip(id).unwrap().dur, 4.0);
        let r = tl.clip(right).unwrap();
        assert_eq!((r.start, r.src_in, r.dur), (4.0, 4.0, 6.0));
        // หัวคลิปแรกดึงกลับเกินต้นไฟล์ไม่ได้
        assert_eq!(tl.trim_head(id, -3.0), 0.0);
        // ท้ายคลิปแรกยืดเกินไฟล์ไม่ได้ (ไฟล์ยาว 10 วิ)
        assert_eq!(tl.trim_tail(right, 5.0), 0.0);
        assert_eq!(tl.trim_head(right, 1.0), 1.0);
        let r = tl.clip(right).unwrap();
        assert_eq!((r.start, r.src_in, r.dur), (4.0, 5.0, 5.0));
        assert!(tl.split(id, 0.01).is_none());
    }

    #[test]
    fn audio_never_overlaps() {
        let mut tl = Timeline::default();
        let a = audio(&mut tl, 5.0);
        let b = audio(&mut tl, 5.0);
        let ida = tl.place_audio(a, 2.0);
        let idb = tl.place_audio(b, 3.0);
        assert_eq!(tl.clip(ida).unwrap().start, 2.0);
        assert_eq!(tl.clip(idb).unwrap().start, 7.0);
        // ยืดท้ายคลิปแรกไปชนคลิปถัดไปไม่ได้ (และเกินความยาวไฟล์ไม่ได้)
        assert_eq!(tl.trim_tail(ida, 3.0), 0.0);
        tl.trim_tail(ida, -2.0);
        assert_eq!(tl.trim_tail(ida, 10.0), 2.0);
        tl.move_free(idb, 0.5);
        assert_eq!(tl.audio[0].id, ida);
        assert_eq!(tl.clip(idb).unwrap().start, 7.0);
        assert_eq!(tl.duration(), 12.0);
    }

    #[test]
    fn images_stretch() {
        let mut tl = Timeline::default();
        let img = tl.new_clip("a.png".into(), "a".into(), MediaKind::Image, &info(0.0));
        let id = tl.insert_main(0, img);
        assert_eq!(tl.clip(id).unwrap().dur, IMAGE_DEFAULT);
        tl.set_duration(id, 12.0);
        assert_eq!(tl.clip(id).unwrap().dur, 12.0);
    }

    #[test]
    fn transitions_overlap_clips() {
        let mut tl = Timeline::default();
        let a = video(&mut tl, 4.0);
        let b = video(&mut tl, 1.0);
        let ida = tl.insert_main(0, a);
        let idb = tl.insert_main(1, b);
        tl.update_clip(ida, |c| c.fx.transition = Some(Transition { kind: TransitionKind::Fade, dur: 2.0 }));
        // คลิปถัดไปยาว 1 วิ ทรานซิชันเลยถูกจำกัดเหลือ 0.5 วิ
        assert_eq!(tl.transition_after(0).unwrap().1, 0.5);
        assert_eq!(tl.clip(idb).unwrap().start, 3.5);
        assert_eq!(tl.duration(), 4.5);
        // แยกคลิปแรก: ทรานซิชันตามไปอยู่กับครึ่งหลัง
        let right = tl.split(ida, 1.0).unwrap();
        assert!(tl.clip(ida).unwrap().fx.transition.is_none());
        assert!(tl.clip(right).unwrap().fx.transition.is_some());
        assert_eq!(tl.duration(), 4.5);
    }

    #[test]
    fn text_tracks_are_free() {
        let mut tl = Timeline::default();
        let t1 = tl.new_text_clip(TextStyle::default(), 3.0);
        let t2 = tl.new_text_clip(TextStyle::default(), 3.0);
        let id1 = tl.place(TrackKind::Text, t1, 1.0);
        let id2 = tl.place(TrackKind::Text, t2, 2.0);
        assert_eq!(tl.clip(id2).unwrap().start, 4.0);
        assert_eq!(tl.find(id1).unwrap().0, TrackKind::Text);
        assert_eq!(tl.duration(), 7.0);
        tl.replace_subtitles(vec![(0.0, 1.5, "สวัสดี".into()), (1.5, 3.0, "ครับ".into()), (3.0, 3.01, "".into())]);
        assert_eq!(tl.subtitles.len(), 2);
        assert_eq!(tl.subtitles[1].start, 1.5);

        // แทนที่เฉพาะช่วง 1..5: บรรทัดแรก (0..1.5) คาบเกี่ยว ถูกลบ
        tl.replace_subtitles(vec![(0.0, 0.8, "ก".into()), (2.0, 3.0, "ข".into()), (6.0, 7.0, "ค".into())]);
        let style = TextStyle { size: 9.0, ..TextStyle::subtitle(String::new()) };
        tl.replace_subtitles_in(1.0, 5.0, vec![(1.2, 2.5, "ใหม่".into())], &style);
        let texts: Vec<&str> = tl.subtitles.iter().map(|c| c.title()).collect();
        assert_eq!(texts, ["ก", "ใหม่", "ค"]);
        assert_eq!(tl.subtitles[1].text.as_ref().unwrap().size, 9.0);

        tl.shift_subtitles(-5.0);
        assert_eq!(tl.subtitles[0].start, 0.0, "ถอยได้ไม่เกินวินาทีที่ 0");
        assert!((tl.subtitles[2].start - 6.0).abs() < 1e-9);
    }

    #[test]
    fn overlay_moves_between_tracks() {
        let mut tl = Timeline::default();
        let a = video(&mut tl, 4.0);
        let b = video(&mut tl, 6.0);
        let ida = tl.insert_main(0, a);
        let idb = tl.insert_main(1, b);
        let pic = tl.new_clip("a.png".into(), "a".into(), MediaKind::Image, &info(0.0));
        let idp = tl.place_overlay(pic, 7.0);
        let p = tl.clip(idp).unwrap();
        assert_eq!((p.start, p.fx.zoom), (7.0, OVERLAY_ZOOM));
        assert_eq!(tl.duration(), 12.0, "ภาพซ้อนยาวเลยแทร็กหลักได้");

        // ย้ายคลิปหลักขึ้นไปซ้อน: แทร็กหลักชิดกันใหม่ ภาพซ้อนวางในที่ว่างใกล้สุด
        tl.update_clip(ida, |c| c.fx.transition = Some(Transition { kind: TransitionKind::Fade, dur: 1.0 }));
        assert!(tl.move_to_track(ida, TrackKind::Overlay, 0.0));
        assert_eq!(tl.clip(idb).unwrap().start, 0.0);
        let a = tl.clip(ida).unwrap();
        assert_eq!((tl.find(ida).unwrap().0, a.start, a.fx.zoom), (TrackKind::Overlay, 0.0, OVERLAY_ZOOM));
        assert!(a.fx.transition.is_none());

        // ย้ายกลับลงแทร็กหลักท้ายสุด ขนาดกลับเป็นเต็มจอ
        assert!(tl.move_to_track(ida, TrackKind::Main, 100.0));
        assert_eq!(tl.main.iter().map(|c| c.id).collect::<Vec<_>>(), [idb, ida]);
        assert_eq!(tl.clip(ida).unwrap().fx.zoom, 1.0);
        assert!(!tl.move_to_track(ida, TrackKind::Audio, 0.0));

        // ภาพซ้อนวางทับเวลากันได้หลายชั้น และคำนวณแถวย่อยถูกต้อง
        let ov2 = video(&mut tl, 4.0);
        let id_ov2 = tl.place_overlay(ov2, 8.0);
        assert_eq!(tl.clip(id_ov2).unwrap().start, 8.0);
        let (n_rows, rows) = tl.overlay_rows();
        assert_eq!(n_rows, 2);
        assert_eq!(rows, vec![0, 1]);
        assert_eq!(tl.clip_at(TrackKind::Overlay, 9.0).unwrap().id, id_ov2);
    }

    #[test]
    fn delete_range_ripples_every_track() {
        let mut tl = Timeline::default();
        let a = video(&mut tl, 10.0);
        tl.insert_main(0, a);
        let b = video(&mut tl, 10.0);
        tl.insert_main(1, b);
        let m = audio(&mut tl, 20.0);
        tl.place_audio(m, 0.0);
        tl.replace_subtitles(vec![(1.0, 2.0, "ก่อน".into()), (5.0, 6.0, "ในช่วง".into()), (13.0, 14.0, "หลัง".into())]);
        // ตัด 4..12: คร่อมรอยต่อคลิปหลัก
        assert!(tl.delete_range(4.0, 12.0));
        assert_eq!(tl.duration(), 12.0);
        assert_eq!(tl.main.len(), 2);
        assert_eq!((tl.main[0].src_in, tl.main[0].dur), (0.0, 4.0));
        assert_eq!((tl.main[1].src_in, tl.main[1].dur, tl.main[1].start), (2.0, 8.0, 4.0));
        // เพลงคร่อมทั้งช่วง: เหลือสองชิ้นต่อกันที่ 4 วิ
        assert_eq!(tl.audio.len(), 2);
        assert_eq!((tl.audio[0].dur, tl.audio[1].start, tl.audio[1].src_in), (4.0, 4.0, 12.0));
        let subs: Vec<(&str, f64)> = tl.subtitles.iter().map(|c| (c.title(), c.start)).collect();
        assert_eq!(subs, [("ก่อน", 1.0), ("หลัง", 5.0)]);
        assert!(!tl.delete_range(3.0, 3.0));
    }
}
