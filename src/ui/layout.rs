//! จัดวางการ์ด (แผง) บนหน้าจอ: ลากจุดจับ ⠿ ที่หัวการ์ดไปวางอีกช่องเพื่อสลับที่
//!
//! แถวบนมี 3 ช่อง (ซ้าย / กลาง / ขวา) ใส่การ์ด สื่อ / เครื่องเล่น / รายละเอียด ในลำดับใดก็ได้
//! ไทม์ไลน์อยู่ด้านล่างหรือด้านบน ค่าที่จัดไว้จำลงไฟล์ ปิดเปิดโปรแกรมใหม่ยังอยู่

use std::path::PathBuf;

use eframe::egui::{self, Align2, Color32, FontId, Id, LayerId, Order, Rect, Sense, Stroke};
use serde::{Deserialize, Serialize};

use crate::app::TadTorApp;
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Card {
    Media,
    Preview,
    Inspector,
    Timeline,
}

impl Card {
    pub fn label(self) -> &'static str {
        match self {
            Card::Media => "สื่อและเครื่องมือ",
            Card::Preview => "เครื่องเล่น",
            Card::Inspector => "รายละเอียด",
            Card::Timeline => "ไทม์ไลน์",
        }
    }

    /// ความกว้างเริ่มต้นและช่วงที่ยอม เมื่ออยู่ช่องซ้าย/ขวา
    fn side_size(self) -> (f32, std::ops::RangeInclusive<f32>) {
        match self {
            Card::Media => (400.0, 300.0..=640.0),
            Card::Preview => (560.0, 320.0..=1100.0),
            Card::Inspector | Card::Timeline => (340.0, 280.0..=520.0),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Card::Media => "card_media",
            Card::Preview => "card_preview",
            Card::Inspector => "card_inspector",
            Card::Timeline => "card_timeline",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// การ์ดในช่อง ซ้าย, กลาง, ขวา
    pub row: [Card; 3],
    pub timeline_top: bool,
    /// ขนาดทั้งหน้าจอ (ตัวอักษร ปุ่ม ฯลฯ) 1 = ปกติ ปรับจากเมนูหรือ Ctrl + / Ctrl − / Ctrl 0
    pub ui_zoom: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self { row: [Card::Media, Card::Preview, Card::Inspector], timeline_top: false, ui_zoom: 1.0 }
    }
}

pub const ZOOMS: [f32; 6] = [0.9, 1.0, 1.1, 1.2, 1.35, 1.5];

impl Layout {
    /// แบบสำเร็จรูปในเมนู
    pub fn presets() -> [(&'static str, Layout); 3] {
        [
            ("ค่าเริ่มต้น (เครื่องเล่นตรงกลาง)", Layout::default()),
            ("เครื่องเล่นขวาสุด", Layout { row: [Card::Media, Card::Inspector, Card::Preview], ..Layout::default() }),
            ("เครื่องเล่นซ้ายสุด", Layout { row: [Card::Preview, Card::Media, Card::Inspector], ..Layout::default() }),
        ]
    }

    fn valid(&self) -> bool {
        let r = self.row;
        r.iter().all(|c| *c != Card::Timeline) && r[0] != r[1] && r[1] != r[2] && r[0] != r[2]
    }

    /// สลับการ์ดสองใบ (ไทม์ไลน์ย้ายได้แค่บน/ล่าง)
    pub fn swap(&mut self, a: Card, b: Card) {
        let (Some(i), Some(j)) = (self.row.iter().position(|c| *c == a), self.row.iter().position(|c| *c == b)) else { return };
        self.row.swap(i, j);
    }
}

fn file() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("TadTor").join("layout.json")
}

pub fn load() -> Layout {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|s| serde_json::from_str::<Layout>(&s).ok())
        .filter(Layout::valid)
        .map(|l| Layout { ui_zoom: l.ui_zoom.clamp(0.7, 2.0), ..l })
        .unwrap_or_default()
}

pub fn save(layout: &Layout) {
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(layout) {
        let _ = std::fs::write(path, s);
    }
}

/// สถานะระหว่างลากการ์ด (ไม่บันทึก)
#[derive(Default)]
pub struct DragState {
    pub card: Option<Card>,
    /// พื้นที่ของแต่ละการ์ดในรอบวาดล่าสุด
    rects: Vec<(Card, Rect)>,
}

/// จุดจับสำหรับลากการ์ด (วาดจุด 6 จุด) วางไว้ที่หัวการ์ด
pub fn grip(app: &mut TadTorApp, ui: &mut egui::Ui, card: Card) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(16.0, 22.0), Sense::drag());
    let dragging = app.card_drag.card == Some(card);
    let color = if dragging || resp.hovered() { theme::ACCENT } else { theme::TEXT_DIM };
    for (dx, dy) in [(-3.0, -5.0), (3.0, -5.0), (-3.0, 0.0), (3.0, 0.0), (-3.0, 5.0), (3.0, 5.0)] {
        ui.painter().circle_filled(rect.center() + egui::vec2(dx, dy), 1.6, color);
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text("ลากไปวางที่การ์ดอื่นเพื่อสลับที่ (เมนู > จัดหน้าจอ มีแบบสำเร็จรูป)");
    if resp.drag_started() {
        app.card_drag.card = Some(card);
    }
    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
}

fn place(app: &mut TadTorApp, ui: &mut egui::Ui, card: Card, side: Option<Side>) {
    let content = |app: &mut TadTorApp, ui: &mut egui::Ui| match card {
        Card::Media => super::media_panel::content(app, ui),
        Card::Preview => super::preview::content(app, ui),
        Card::Inspector => super::inspector::content(app, ui),
        Card::Timeline => super::timeline::content(app, ui),
    };
    let rect = match side {
        Some(side) => {
            let (size, range) = card.side_size();
            let panel = match side {
                Side::Left => egui::Panel::left(card.id()),
                Side::Right => egui::Panel::right(card.id()),
            };
            panel
                .resizable(true)
                .default_size(size)
                .size_range(range)
                .show_separator_line(false)
                .frame(super::panel_frame())
                .show(ui, |ui| content(app, ui))
                .response
                .rect
        }
        None => egui::CentralPanel::no_frame().frame(super::panel_frame()).show(ui, |ui| content(app, ui)).response.rect,
    };
    app.card_drag.rects.push((card, rect));
}

/// วางการ์ดทั้งหมดตามที่จัดไว้ (เรียกหลังแถบบนสุด)
pub fn show(app: &mut TadTorApp, ui: &mut egui::Ui) {
    // ย่อ/ขยายด้วย Ctrl + / Ctrl − ของ eframe: จำค่าไว้
    let z = ui.ctx().zoom_factor();
    if (z - app.layout.ui_zoom).abs() > 1e-3 {
        app.layout.ui_zoom = z;
        save(&app.layout);
    }
    app.card_drag.rects.clear();
    let timeline = if app.layout.timeline_top { egui::Panel::top(Card::Timeline.id()) } else { egui::Panel::bottom(Card::Timeline.id()) };
    let rect = timeline
        .resizable(true)
        .default_size(330.0)
        .size_range(200.0..=640.0)
        .show_separator_line(false)
        .frame(super::panel_frame())
        .show(ui, |ui| super::timeline::content(app, ui))
        .response
        .rect;
    app.card_drag.rects.push((Card::Timeline, rect));
    let [left, center, right] = app.layout.row;
    place(app, ui, left, Some(Side::Left));
    place(app, ui, right, Some(Side::Right));
    place(app, ui, center, None);
    drag_overlay(app, ui.ctx());
}

/// ระหว่างลาก: ไฮไลต์ช่องที่จะวาง ปล่อยเมาส์แล้วสลับที่
fn drag_overlay(app: &mut TadTorApp, ctx: &egui::Context) {
    let Some(dragged) = app.card_drag.card else { return };
    let pointer = ctx.pointer_latest_pos();
    let released = ctx.input(|i| !i.pointer.primary_down());
    let rects = app.card_drag.rects.clone();
    let row_area = rects.iter().filter(|(c, _)| *c != Card::Timeline).map(|(_, r)| *r).reduce(|a, b| a.union(b));

    // เป้าหมาย: การ์ดใต้เมาส์ (ลากไทม์ไลน์ = ครึ่งบน/ล่างของแถวการ์ด)
    let target: Option<(Rect, Drop)> = pointer.and_then(|p| {
        if dragged == Card::Timeline {
            let area = row_area?;
            let top = p.y < area.center().y;
            let zone = if top {
                Rect::from_min_max(area.min, egui::pos2(area.max.x, area.min.y + 90.0))
            } else {
                Rect::from_min_max(egui::pos2(area.min.x, area.max.y - 90.0), area.max)
            };
            Some((zone, Drop::Timeline { top }))
        } else {
            rects.iter().find(|(c, r)| *c != Card::Timeline && r.contains(p)).map(|(c, r)| (*r, Drop::Swap(*c)))
        }
    });

    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("card_drop")));
    for (c, r) in &rects {
        if *c == dragged {
            painter.rect_filled(r.shrink(3.0), 8, Color32::from_black_alpha(90));
        }
    }
    if let Some((r, drop)) = target {
        let useful = !matches!(drop, Drop::Swap(c) if c == dragged);
        if useful {
            painter.rect_filled(r.shrink(3.0), 8, theme::ACCENT.gamma_multiply(0.18));
            painter.rect_stroke(r.shrink(3.0), 8, Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
            let text = match drop {
                Drop::Swap(c) => format!("สลับกับ \"{}\"", c.label()),
                Drop::Timeline { top: true } => "ไทม์ไลน์ไว้ด้านบน".to_owned(),
                Drop::Timeline { top: false } => "ไทม์ไลน์ไว้ด้านล่าง".to_owned(),
            };
            painter.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(16.0), theme::TEXT);
        }
    }
    if let Some(p) = pointer {
        let label = format!("⠿ {}", dragged.label());
        let galley = painter.layout_no_wrap(label, FontId::proportional(13.0), Color32::BLACK);
        let box_rect = Rect::from_min_size(p + egui::vec2(14.0, 10.0), galley.size() + egui::vec2(16.0, 8.0));
        painter.rect_filled(box_rect, 6, theme::ACCENT);
        painter.galley(box_rect.min + egui::vec2(8.0, 4.0), galley, Color32::BLACK);
    }
    ctx.request_repaint();

    if released {
        app.card_drag.card = None;
        let before = app.layout.clone();
        match target.map(|t| t.1) {
            Some(Drop::Swap(c)) => app.layout.swap(dragged, c),
            Some(Drop::Timeline { top }) => app.layout.timeline_top = top,
            None => {}
        }
        if app.layout != before {
            save(&app.layout);
        }
    }
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy)]
enum Drop {
    Swap(Card),
    Timeline { top: bool },
}

/// เมนูจัดหน้าจอ (ในเมนูหลัก)
pub fn menu(app: &mut TadTorApp, ui: &mut egui::Ui) {
    for (name, preset) in Layout::presets() {
        let current = app.layout.row == preset.row;
        if ui.add(egui::Button::selectable(current, name)).clicked() {
            app.layout.row = preset.row;
            save(&app.layout);
            ui.close();
        }
    }
    ui.separator();
    let mut top = app.layout.timeline_top;
    if ui.checkbox(&mut top, "ไทม์ไลน์ไว้ด้านบน").changed() {
        app.layout.timeline_top = top;
        save(&app.layout);
    }
    ui.separator();
    ui.label(egui::RichText::new("หรือลากจุด ⠿ ที่หัวการ์ดไปวางที่การ์ดอื่น").color(theme::TEXT_DIM).size(12.5));
    ui.separator();
    ui.label("ขนาดตัวอักษรและหน้าจอ");
    ui.horizontal(|ui| {
        for z in ZOOMS {
            let on = (app.layout.ui_zoom - z).abs() < 0.01;
            if ui.add(egui::Button::selectable(on, format!("{:.0}%", z * 100.0))).clicked() {
                ui.ctx().set_zoom_factor(z);
            }
        }
    });
    ui.label(egui::RichText::new("ปุ่มลัด: Ctrl + ขยาย • Ctrl − ย่อ • Ctrl 0 ปกติ").color(theme::TEXT_DIM).size(12.5));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_keeps_layout_valid() {
        let mut l = Layout::default();
        l.swap(Card::Preview, Card::Inspector);
        assert_eq!(l.row, [Card::Media, Card::Inspector, Card::Preview]);
        l.swap(Card::Timeline, Card::Media);
        assert!(l.valid());
        assert!(!Layout { row: [Card::Media, Card::Media, Card::Preview], ..Layout::default() }.valid());
        // ไฟล์เก่าที่ยังไม่มี ui_zoom
        let old: Layout = serde_json::from_str(r#"{"row":["Media","Inspector","Preview"],"timeline_top":false}"#).unwrap();
        assert_eq!(old.ui_zoom, 1.0);
        let back: Layout = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(back, l);
    }
}
