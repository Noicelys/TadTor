//! การตั้งค่าโปรแกรม (จำไว้ข้ามการเปิดโปรแกรม) เก็บที่ `%LOCALAPPDATA%\TadTor\settings.json`
//!
//! การจัดหน้าจอ/ขนาดตัวอักษรอยู่แยกใน `layout.json` (ดู `ui::layout`)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::project::{AspectRatio, Project};
use crate::ui::subtitle_dialog::SubtitleSettings;

/// จำนวนงานล่าสุดที่จำไว้
const RECENT_MAX: usize = 10;

/// ความละเอียดของภาพพรีวิว (ไม่มีผลกับไฟล์ที่ส่งออก)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewQuality {
    Smooth,
    Balanced,
    Sharp,
}

impl PreviewQuality {
    pub const ALL: [PreviewQuality; 3] = [PreviewQuality::Smooth, PreviewQuality::Balanced, PreviewQuality::Sharp];

    pub fn label(self) -> &'static str {
        match self {
            PreviewQuality::Smooth => "ลื่น (360p)",
            PreviewQuality::Balanced => "สมดุล (540p)",
            PreviewQuality::Sharp => "คมชัด (720p)",
        }
    }

    /// ด้านสั้นของภาพพรีวิว (พิกเซล)
    pub fn short_side(self) -> u32 {
        match self {
            PreviewQuality::Smooth => 360,
            PreviewQuality::Balanced => 540,
            PreviewQuality::Sharp => 720,
        }
    }
}

/// ค่าเริ่มต้นของโปรเจกต์ใหม่
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewProject {
    pub aspect: AspectRatio,
    pub quality: u32,
    pub fps: u32,
}

impl Default for NewProject {
    fn default() -> Self {
        let p = Project::default();
        Self { aspect: p.aspect, quality: p.quality, fps: p.fps }
    }
}

impl NewProject {
    pub fn project(&self) -> Project {
        Project { aspect: self.aspect, quality: self.quality, fps: self.fps, ..Project::default() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// บันทึกอัตโนมัติ (งานที่ยังไม่เคยบันทึกเก็บเป็นไฟล์สำรองไว้กู้คืน)
    pub autosave: bool,
    pub autosave_minutes: u32,
    /// เปิดงานล่าสุดให้เองตอนเปิดโปรแกรม
    pub open_last_project: bool,
    /// งานที่เปิด/บันทึกล่าสุด (ใหม่สุดอยู่หน้า)
    pub recent: Vec<PathBuf>,
    pub new_project: NewProject,
    pub preview: PreviewQuality,
    /// โฟลเดอร์ส่งออกเริ่มต้น (None = ข้างไฟล์โปรเจกต์ หรือโฟลเดอร์ Videos)
    pub export_dir: Option<PathBuf>,
    /// ค่าที่ตั้งไว้ในแท็บซับไตเติล
    pub subtitles: SubtitleSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autosave: true,
            autosave_minutes: 3,
            open_last_project: true,
            recent: Vec::new(),
            new_project: NewProject::default(),
            preview: PreviewQuality::Balanced,
            export_dir: None,
            subtitles: SubtitleSettings::default(),
        }
    }
}

impl Settings {
    /// จำงานนี้ไว้หน้าสุดของรายการงานล่าสุด
    pub fn remember(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(RECENT_MAX);
    }
}

/// โฟลเดอร์เก็บค่าตั้งของโปรแกรม
pub fn dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("TadTor")
}

fn file() -> PathBuf {
    dir().join("settings.json")
}

/// ไฟล์สำรองของงานที่ยังไม่เคยบันทึก (มีอยู่ตอนเปิดโปรแกรม = ครั้งก่อนปิดกะทันหัน)
pub fn recovery_file() -> PathBuf {
    dir().join("recovery.tadtor")
}

pub fn load() -> Settings {
    std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(s: &Settings) {
    let _ = std::fs::create_dir_all(dir());
    if let Ok(text) = serde_json::to_string_pretty(s) {
        let tmp = file().with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, file());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_is_unique_and_capped() {
        let mut s = Settings::default();
        for i in 0..15 {
            s.remember(Path::new(&format!("{i}.tadtor")));
        }
        s.remember(Path::new("3.tadtor"));
        assert_eq!(s.recent.len(), RECENT_MAX);
        assert_eq!(s.recent[0], Path::new("3.tadtor"));
        assert_eq!(s.recent.iter().filter(|p| *p == Path::new("3.tadtor")).count(), 1);
    }

    #[test]
    fn old_or_partial_file_loads() {
        let s: Settings = serde_json::from_str(r#"{"autosave":false,"new_project":{"fps":60}}"#).unwrap();
        assert!(!s.autosave && s.open_last_project);
        assert_eq!((s.new_project.fps, s.new_project.quality), (60, 1080));
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
