//! บันทึก / เปิดโปรเจกต์เป็นไฟล์ .tadtor (ข้างในเป็น JSON อ่านด้วยตาได้)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::project::Project;
use super::timeline::Timeline;

pub const EXTENSION: &str = "tadtor";
const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub project: Project,
    /// ไฟล์ในคลังสื่อ (เก็บแค่ที่อยู่ ไม่ได้คัดลอกไฟล์)
    pub media: Vec<PathBuf>,
    pub timeline: Timeline,
}

pub fn save(path: &Path, project: &Project, media: Vec<PathBuf>, timeline: &Timeline) -> Result<(), String> {
    let file = ProjectFile { version: FORMAT_VERSION, project: project.clone(), media, timeline: timeline.clone() };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    // เขียนไฟล์ชั่วคราวก่อนแล้วค่อยเปลี่ยนชื่อ ไฟล์เดิมจะไม่พังถ้าเครื่องดับกลางคัน
    let tmp = path.with_extension("tadtor.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn load(path: &Path) -> Result<ProjectFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file: ProjectFile = serde_json::from_str(&text).map_err(|e| format!("ไฟล์โปรเจกต์เสียหาย: {e}"))?;
    if file.version > FORMAT_VERSION {
        return Err("ไฟล์นี้สร้างจาก TadTor รุ่นใหม่กว่า".to_owned());
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::media::{MediaInfo, MediaKind};

    #[test]
    fn roundtrip() {
        let mut tl = Timeline::default();
        let info = MediaInfo { duration: Some(3.0), width: 10, height: 10, fps: None, has_video: true, has_audio: false };
        let c = tl.new_clip("ก.mp4".into(), "ก".into(), MediaKind::Video, &info);
        tl.insert_main(0, c);
        let dir = std::env::temp_dir().join("tadtor-test-roundtrip.tadtor");
        save(&dir, &Project::default(), vec!["ก.mp4".into()], &tl).unwrap();
        let back = load(&dir).unwrap();
        assert_eq!(back.timeline, tl);
        assert_eq!(back.project, Project::default());
        let _ = std::fs::remove_file(dir);
    }
}
