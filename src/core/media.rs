use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::icons;

/// ประเภทไฟล์สื่อ ดูจากนามสกุลไฟล์
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
    /// คลิปข้อความ/ซับไตเติล (ไม่มีไฟล์)
    Text,
}

impl MediaKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "wmv" | "flv" | "mts" | "ts" => Some(MediaKind::Video),
            "mp3" | "wav" | "aac" | "m4a" | "flac" | "ogg" | "opus" | "wma" => Some(MediaKind::Audio),
            "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tif" | "tiff" => Some(MediaKind::Image),
            _ => None,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            MediaKind::Video => icons::VIDEO,
            MediaKind::Audio => icons::MUSIC_NOTE,
            MediaKind::Image => icons::IMAGE,
            MediaKind::Text => icons::TEXT_T,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Video => "วิดีโอ",
            MediaKind::Audio => "เสียง",
            MediaKind::Image => "รูปภาพ",
            MediaKind::Text => "ข้อความ",
        }
    }

    /// ภาพ/วิดีโอไปอยู่แทร็กหลัก ส่วนไฟล์เสียงไปแทร็กเสียง
    pub fn is_visual(self) -> bool {
        matches!(self, MediaKind::Video | MediaKind::Image)
    }

    pub const VIDEO_EXT: &'static [&'static str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "mts", "ts"];
    pub const AUDIO_EXT: &'static [&'static str] = &["mp3", "wav", "aac", "m4a", "flac", "ogg", "opus", "wma"];
    pub const IMAGE_EXT: &'static [&'static str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];
}

/// ข้อมูลไฟล์ที่อ่านได้จาก ffprobe
#[derive(Clone, Debug, PartialEq)]
pub struct MediaInfo {
    /// ความยาว (วินาที) — รูปภาพเป็น None
    pub duration: Option<f64>,
    /// ขนาดภาพหลังหมุนตามข้อมูลในไฟล์แล้ว (วิดีโอจากมือถือมักถ่ายแนวตั้ง)
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub has_video: bool,
    pub has_audio: bool,
}

/// สถานะการอ่านไฟล์ (ทำเบื้องหลัง)
#[derive(Clone, Debug)]
pub enum MediaState {
    Probing,
    Ready(MediaInfo),
    Failed(String),
}

/// ไฟล์หนึ่งไฟล์ในคลังสื่อ
pub struct MediaItem {
    pub path: PathBuf,
    pub name: String,
    pub kind: MediaKind,
    pub state: MediaState,
}

impl MediaItem {
    pub fn from_path(path: PathBuf) -> Option<Self> {
        let kind = MediaKind::from_path(&path)?;
        let name = file_name(&path);
        Some(Self { path, name, kind, state: MediaState::Probing })
    }

    pub fn info(&self) -> Option<&MediaInfo> {
        match &self.state {
            MediaState::Ready(info) => Some(info),
            _ => None,
        }
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// คลังสื่อของโปรเจกต์
#[derive(Default)]
pub struct MediaLibrary {
    pub items: Vec<MediaItem>,
}

impl MediaLibrary {
    /// เพิ่มไฟล์ (ข้ามไฟล์ซ้ำและไฟล์ที่ไม่รองรับ) คืนรายการไฟล์ที่เพิ่มได้
    pub fn add_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
        let mut added = Vec::new();
        for path in paths {
            if self.items.iter().any(|it| it.path == path) {
                continue;
            }
            if let Some(item) = MediaItem::from_path(path) {
                added.push(item.path.clone());
                self.items.push(item);
            }
        }
        added
    }

    pub fn get(&self, path: &Path) -> Option<&MediaItem> {
        self.items.iter().find(|it| it.path == path)
    }

    pub fn get_mut(&mut self, path: &Path) -> Option<&mut MediaItem> {
        self.items.iter_mut().find(|it| it.path == path)
    }
}
