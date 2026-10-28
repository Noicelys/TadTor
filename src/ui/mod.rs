//! หน้าจอทั้งหมด แยกเป็นไฟล์ละส่วน

pub mod dialogs;
pub mod export_dialog;
pub mod fader;
pub mod inspector;
pub mod layout;
pub mod media_panel;
pub mod preview;
pub mod subtitle_dialog;
pub mod timeline;
pub mod tools_panel;
pub mod top_bar;

use eframe::egui::{self, Color32, Rect, Shape, Stroke};

use crate::theme;

/// กรอบของแผงหลัก (มีขอบมนและช่องว่างรอบ ๆ ให้ดูเป็นการ์ดแยกกัน)
pub fn panel_frame() -> egui::Frame {
    egui::Frame::new().fill(theme::PANEL).corner_radius(8).outer_margin(egui::Margin::same(3)).inner_margin(egui::Margin::same(0))
}

/// แถบหัวแผงเล็ก ๆ แบบมีชื่อ
pub fn panel_header(ui: &mut egui::Ui, title: &str, add_right: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::PANEL_HEADER)
        .corner_radius(egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).strong().size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add_right);
            });
        });
}

/// วาดกรอบเส้นประรอบสี่เหลี่ยม (ใช้กับพื้นที่ลากไฟล์มาวาง)
pub fn dashed_rect(painter: &egui::Painter, rect: Rect, color: Color32) {
    let pts = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
    painter.extend(Shape::dashed_line(&pts, Stroke::new(1.0, color), 6.0, 4.0));
}

/// ตัดข้อความยาวให้พอดีกับจำนวนตัวอักษร
pub fn ellipsize(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let mut s: String = text.chars().take(max_chars.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

/// จุดสีบอกสถานะ
pub fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// สี่เหลี่ยมที่ใหญ่ที่สุดในพื้นที่ ตามอัตราส่วนที่ต้องการ (จัดกึ่งกลาง)
pub fn fit_rect(area: Rect, ratio: f32) -> Rect {
    let (aw, ah) = (area.width().max(1.0), area.height().max(1.0));
    let (w, h) = if aw / ah > ratio { (ah * ratio, ah) } else { (aw, aw / ratio) };
    Rect::from_center_size(area.center(), egui::vec2(w, h))
}
