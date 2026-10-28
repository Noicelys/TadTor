//! ไอคอนของโปรแกรม: ฟอนต์ Phosphor Icons (MIT, phosphoricons.com) ฝังอยู่ในโปรแกรม
//!
//! ใช้เหมือนตัวอักษรธรรมดา เช่น `format!("{} บันทึก", icons::FLOPPY_DISK)`
//! อยากได้ไอคอนเพิ่ม: หารหัสจาก https://phosphoricons.com แล้วเพิ่มค่าคงที่ที่นี่

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

const REGULAR: &[u8] = include_bytes!("../assets/Phosphor.ttf");
const FILL: &[u8] = include_bytes!("../assets/Phosphor-Fill.ttf");

/// ตระกูลฟอนต์ไอคอนแบบทึบ (ใช้กับปุ่มเล่น)
pub fn fill_family() -> FontFamily {
    FontFamily::Name("phosphor-fill".into())
}

pub fn fill(size: f32) -> egui::FontId {
    egui::FontId::new(size, fill_family())
}

/// ใส่ฟอนต์ไอคอนต่อจากฟอนต์ตัวหนังสือ (ต้องอยู่หลังฟอนต์หลัก ไม่งั้นตัวอักษรอังกฤษบางตัวจะหาย)
pub fn install(fonts: &mut FontDefinitions) {
    fonts.font_data.insert("phosphor".to_owned(), std::sync::Arc::new(FontData::from_static(REGULAR)));
    fonts.font_data.insert("phosphor-fill".to_owned(), std::sync::Arc::new(FontData::from_static(FILL)));
    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    let at = prop.len().min(1);
    prop.insert(at, "phosphor".to_owned());
    let mut fill_keys = vec!["phosphor-fill".to_owned()];
    fill_keys.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts.families.insert(fill_family(), fill_keys);
}

pub const ARROW_LINE_LEFT: &str = "\u{E062}";
pub const ARROW_LINE_RIGHT: &str = "\u{E064}";
pub const ARROW_U_UP_LEFT: &str = "\u{E08A}";
pub const ARROW_U_UP_RIGHT: &str = "\u{E08C}";
pub const ARROWS_OUT_LINE_HORIZONTAL: &str = "\u{E534}";
pub const CIRCLE_HALF: &str = "\u{E18C}";
pub const EXPORT: &str = "\u{EAF0}";
pub const FILE_PLUS: &str = "\u{E236}";
pub const FILM_STRIP: &str = "\u{E792}";
pub const FLOPPY_DISK: &str = "\u{E248}";
pub const FOLDER_OPEN: &str = "\u{E256}";
pub const FOLDER_SIMPLE_PLUS: &str = "\u{E25E}";
pub const IMAGE: &str = "\u{E2CA}";
pub const INTERSECT: &str = "\u{E2D2}";
pub const LIST: &str = "\u{E2F0}";
pub const MAGNIFYING_GLASS_MINUS: &str = "\u{E30E}";
pub const MAGNIFYING_GLASS_PLUS: &str = "\u{E310}";
pub const MUSIC_NOTE: &str = "\u{E33C}";
pub const MUSIC_NOTES: &str = "\u{E340}";
pub const PAUSE: &str = "\u{E39E}";
pub const PLAY: &str = "\u{E3D0}";
pub const PLUS: &str = "\u{E3D4}";
pub const SCISSORS: &str = "\u{EAE0}";
pub const SIGN_OUT: &str = "\u{E42A}";
pub const SLIDERS_HORIZONTAL: &str = "\u{E434}";
pub const SPARKLE: &str = "\u{E6A2}";
pub const SPEAKER_HIGH: &str = "\u{E44A}";
pub const SPEAKER_SLASH: &str = "\u{E45A}";
pub const TEXT_T: &str = "\u{E48A}";
pub const TRASH: &str = "\u{E4A6}";
pub const TRAY_ARROW_DOWN: &str = "\u{E010}";
pub const VIDEO: &str = "\u{E740}";
pub const WARNING: &str = "\u{E4E0}";
pub const ARROW_COUNTER_CLOCKWISE: &str = "\u{E038}";
pub const CIRCLE_HALF_TILT: &str = "\u{E18E}";
pub const CLOUD_ARROW_DOWN: &str = "\u{E1AC}";
pub const DIAMOND: &str = "\u{E1EC}";
pub const DROP: &str = "\u{E210}";
pub const EYE: &str = "\u{E220}";
pub const EYE_SLASH: &str = "\u{E224}";
pub const FILE_ARROW_DOWN: &str = "\u{E232}";
pub const FILE_ARROW_UP: &str = "\u{E61E}";
pub const MAGIC_WAND: &str = "\u{E6B6}";
pub const MICROPHONE: &str = "\u{E326}";
pub const PALETTE: &str = "\u{E6C8}";
pub const SUBTITLES: &str = "\u{E1A8}";
pub const SUN: &str = "\u{E472}";
pub const TEXT_AA: &str = "\u{E6EE}";
pub const THERMOMETER: &str = "\u{E5C6}";
pub const X_CIRCLE: &str = "\u{E4F8}";
