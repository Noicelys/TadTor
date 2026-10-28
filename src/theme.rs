//! สีและสไตล์ของโปรแกรม (ธีมมืดแบบโปรแกรมตัดต่อ)

use eframe::egui::{self, Color32, CornerRadius, FontData, FontFamily, Stroke};

// ---- พาเลตสี ----
pub const BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x14); // พื้นหลังสุด (ช่องว่างระหว่างแผง)
pub const PANEL: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1f); // พื้นแผง
pub const PANEL_HEADER: Color32 = Color32::from_rgb(0x24, 0x24, 0x28); // แถบหัวแผง
pub const SURFACE: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2f); // ปุ่ม / ช่องกรอก
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x34, 0x34, 0x3a);
pub const BORDER: Color32 = Color32::from_rgb(0x2e, 0x2e, 0x33);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xea);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0xa4, 0xa4, 0xad); // ตัวหนังสือรอง (สว่างพอให้อ่านสบายบนพื้นมืด)
pub const ACCENT: Color32 = Color32::from_rgb(0x22, 0xc7, 0xd6); // สีหลัก (ฟ้าอมเขียว)
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x12, 0x6e, 0x78);
pub const PLAYHEAD: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf2);
pub const OK: Color32 = Color32::from_rgb(0x4c, 0xd1, 0x7a);
pub const WARN: Color32 = Color32::from_rgb(0xf2, 0xb8, 0x3c);
pub const ERROR: Color32 = Color32::from_rgb(0xf0, 0x5a, 0x5a);
pub const TRACK_BG: Color32 = Color32::from_rgb(0x17, 0x17, 0x1a);

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.panel_fill = PANEL;
        v.window_fill = PANEL_HEADER;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.extreme_bg_color = Color32::from_rgb(0x0c, 0x0c, 0x0e);
        v.faint_bg_color = PANEL_HEADER;
        v.override_text_color = Some(TEXT);
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = ACCENT_DARK;
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.window_corner_radius = CornerRadius::same(8);
        v.menu_corner_radius = CornerRadius::same(6);

        let r = CornerRadius::same(5);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = PANEL;
        w.noninteractive.weak_bg_fill = PANEL;
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
        w.noninteractive.corner_radius = r;

        w.inactive.bg_fill = SURFACE;
        w.inactive.weak_bg_fill = SURFACE;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.inactive.corner_radius = r;

        w.hovered.bg_fill = SURFACE_HOVER;
        w.hovered.weak_bg_fill = SURFACE_HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x48, 0x48, 0x50));
        w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        w.hovered.corner_radius = r;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = ACCENT_DARK;
        w.active.weak_bg_fill = ACCENT_DARK;
        w.active.bg_stroke = Stroke::new(1.0, ACCENT);
        w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        w.active.corner_radius = r;
        w.active.expansion = 0.0;

        w.open.bg_fill = SURFACE_HOVER;
        w.open.weak_bg_fill = SURFACE_HOVER;
        w.open.corner_radius = r;

        // ขนาดตัวอักษร: ภาษาไทยมีสระบน-ล่าง ขนาดเริ่มต้นของ egui (12.5) เล็กไปจนอ่านยาก
        use egui::{FontFamily::Monospace, FontFamily::Proportional, FontId, TextStyle};
        style.text_styles = [
            (TextStyle::Small, FontId::new(12.0, Proportional)),
            (TextStyle::Body, FontId::new(14.5, Proportional)),
            (TextStyle::Button, FontId::new(14.5, Proportional)),
            (TextStyle::Heading, FontId::new(19.0, Proportional)),
            (TextStyle::Monospace, FontId::new(13.5, Monospace)),
        ]
        .into();

        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 7.0);
        s.button_padding = egui::vec2(10.0, 5.0);
        s.interact_size.y = 28.0;
    });
}

/// ใช้ฟอนต์ UI ของ Windows ที่รองรับภาษาไทยเป็นฟอนต์หลัก
/// (ถ้าหาไม่เจอ eframe จะหาฟอนต์ไทยจากระบบให้เองอัตโนมัติอยู่แล้ว) แล้วต่อด้วยฟอนต์ไอคอน
pub fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = [
        r"C:\Windows\Fonts\LeelawUI.ttf",                          // Leelawadee UI (Windows 10/11)
        r"C:\Windows\Fonts\leelawad.ttf",                          // Leelawadee
        r"C:\Windows\Fonts\tahoma.ttf",                            // Tahoma (มีภาษาไทย)
        "/usr/share/fonts/truetype/noto/NotoSansThai-Regular.ttf", // Linux (ไว้ทดสอบ)
        "/usr/share/fonts/truetype/tlwg/Loma.ttf",
    ];
    if let Some(bytes) = candidates.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("ui-thai".to_owned(), std::sync::Arc::new(FontData::from_owned(bytes)));
        // ใส่ไว้ลำดับแรก = ฟอนต์หลัก, ฟอนต์เดิมของ egui ยังเป็นตัวสำรอง
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui-thai".to_owned());
    }
    crate::icons::install(&mut fonts);
    ctx.set_fonts(fonts);
}
