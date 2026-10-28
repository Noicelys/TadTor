//! รายชื่อฟอนต์ที่ติดตั้งในเครื่อง (อ่านชื่อจากไฟล์ด้วย ttf-parser) ใช้เลือกฟอนต์ของข้อความ
//!
//! สแกนครั้งเดียว (หลายร้อยไฟล์ ใช้เวลาราว 1 วินาที) โปรแกรมเริ่มสแกนเบื้องหลังตั้งแต่เปิดด้วย `warm_up`

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ttf_parser::name_id;

pub struct Face {
    pub path: PathBuf,
    /// 100..900 (400 = ปกติ, 700 = หนา)
    pub weight: u16,
    pub italic: bool,
}

pub struct Family {
    pub name: String,
    /// มีตัวอักษรไทย
    pub thai: bool,
    pub faces: Vec<Face>,
}

impl Family {
    /// แบบที่ใกล้ที่สุดกับที่ต้องการ (ถ้าไม่มีตัวหนา/ตัวเอียงก็ได้แบบปกติ)
    pub fn face(&self, bold: bool, italic: bool) -> &Face {
        let want: i32 = if bold { 700 } else { 400 };
        self.faces
            .iter()
            .min_by_key(|f| (f.italic != italic) as u32 * 10_000 + (f.weight as i32 - want).unsigned_abs())
            .expect("ทุกตระกูลมีอย่างน้อยหนึ่งแบบ")
    }

    pub fn has_bold(&self) -> bool {
        self.faces.iter().any(|f| f.weight >= 600)
    }

    pub fn has_italic(&self) -> bool {
        self.faces.iter().any(|f| f.italic)
    }
}

static CATALOG: OnceLock<Vec<Family>> = OnceLock::new();

/// เริ่มสแกนฟอนต์เบื้องหลัง หน้าจอจะได้ไม่ค้างตอนเปิดรายชื่อครั้งแรก
pub fn warm_up() {
    std::thread::spawn(families);
}

/// สแกนเสร็จแล้วหรือยัง (หน้าจอใช้เช็กก่อน จะได้ไม่รอ)
pub fn ready() -> bool {
    CATALOG.get().is_some()
}

/// ทุกตระกูลฟอนต์ เรียงตามชื่อ (รอให้สแกนเสร็จถ้ายังไม่เสร็จ)
pub fn families() -> &'static [Family] {
    CATALOG.get_or_init(scan)
}

pub fn find(name: &str) -> Option<&'static Family> {
    families().iter().find(|f| f.name == name)
}

fn font_dirs() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from(r"C:\Windows\Fonts")];
    // ฟอนต์ที่ผู้ใช้ติดตั้งเอง (ไม่ได้ติดตั้งให้ทุกคน)
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        v.push(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"));
    }
    v
}

fn scan() -> Vec<Family> {
    let mut families: Vec<Family> = Vec::new();
    for dir in font_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for path in entries.flatten().map(|e| e.path()) {
            // .ttc (หลายฟอนต์ในไฟล์เดียว) ใช้กับ FFmpeg ได้แค่ตัวแรก จึงข้าม
            let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            if !matches!(ext.as_deref(), Some("ttf" | "otf")) {
                continue;
            }
            let Some((name, face, thai)) = read_face(&path) else { continue };
            match families.iter_mut().find(|f| f.name == name) {
                Some(f) => {
                    f.thai |= thai;
                    f.faces.push(face);
                }
                None => families.push(Family { name, thai, faces: vec![face] }),
            }
        }
    }
    families.sort_by_key(|f| f.name.to_lowercase());
    families
}

fn read_face(path: &Path) -> Option<(String, Face, bool)> {
    let data = std::fs::read(path).ok()?;
    let face = ttf_parser::Face::parse(&data, 0).ok()?;
    let name = family_name(&face)?;
    let thai = face.glyph_index('ก').is_some();
    let weight = face.weight().to_number();
    Some((name, Face { path: path.to_owned(), weight, italic: face.is_italic() }, thai))
}

/// ชื่อตระกูล (ชื่อแบบ typographic ก่อน เช่น "Segoe UI" แทน "Segoe UI Semibold") ภาษาอังกฤษก่อน
fn family_name(face: &ttf_parser::Face) -> Option<String> {
    face.names()
        .into_iter()
        .filter_map(|n| {
            let rank = match n.name_id {
                name_id::TYPOGRAPHIC_FAMILY => 0,
                name_id::FAMILY => 2,
                _ => return None,
            } + (n.language_id != 0x0409) as u8;
            Some((rank, n.to_string()?))
        })
        .filter(|(_, s)| !s.trim().is_empty())
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, s)| s.trim().to_owned())
}
