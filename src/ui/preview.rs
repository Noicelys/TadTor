use eframe::egui::{self, Align2, FontId, RichText, Sense};

use crate::app::TadTorApp;
use crate::core::project::AspectRatio;
use crate::core::time::format_timecode;
use crate::{icons, theme};

const CONTROLS_H: f32 = 44.0;

/// การ์ดเครื่องเล่น (วางในช่องไหนก็ได้ ดู `layout`)
pub fn content(app: &mut TadTorApp, ui: &mut egui::Ui) {
    super::panel_header(ui, "เครื่องเล่น", |ui| super::layout::grip(app, ui, super::layout::Card::Preview));

    // ---- พื้นที่ภาพ ----
    let avail = ui.available_size();
    let area_h = (avail.y - CONTROLS_H).max(80.0);
    let (area, resp) = ui.allocate_exact_size(egui::vec2(avail.x, area_h), Sense::click());
    let canvas = super::fit_rect(area.shrink(16.0), app.project.aspect.ratio());

    let p = ui.painter();
    p.rect_filled(canvas, 2, egui::Color32::BLACK);
    match &app.preview_tex {
        Some(tex) => {
            let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
            p.image(tex.id(), canvas, uv, egui::Color32::WHITE);
        }
        None => {
            let (w, h) = app.project.resolution();
            let hint = if app.timeline.main.is_empty() {
                "ลากวิดีโอลงไทม์ไลน์เพื่อดูพรีวิว"
            } else {
                "กำลังโหลดภาพ…"
            };
            p.text(canvas.center() - egui::vec2(0.0, 10.0), Align2::CENTER_CENTER, hint, FontId::proportional(15.0), theme::TEXT_DIM);
            p.text(
                canvas.center() + egui::vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                format!("{w} × {h}  •  {} fps", app.project.fps),
                FontId::proportional(13.0),
                theme::TEXT_DIM.gamma_multiply(0.7),
            );
        }
    }
    if resp.clicked() {
        let ctx = ui.ctx().clone();
        app.toggle_play(&ctx);
    }

    // ---- ปุ่มควบคุม ----
    controls(app, ui);
}

fn controls(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let fps = app.project.fps;
    let rect = ui.available_rect_before_wrap();
    let row = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), CONTROLS_H));
    let mut child = ui.new_child(
        egui::UiBuilder::new().max_rect(row.shrink2(egui::vec2(14.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ui = &mut child;
    let ctx = ui.ctx().clone();

    // เวลาปัจจุบัน / เวลาทั้งหมด
    ui.label(RichText::new(format_timecode(app.playhead, fps)).monospace().color(theme::ACCENT).size(13.0));
    ui.label(RichText::new(format!("/ {}", format_timecode(app.timeline.duration(), fps))).monospace().color(theme::TEXT_DIM).size(13.0));

    // ปุ่มเล่นตรงกลาง
    let center_x = row.center().x;
    let btn_rect = egui::Rect::from_center_size(egui::pos2(center_x, row.center().y), egui::vec2(34.0, 34.0));
    let icon = if app.playing { icons::PAUSE } else { icons::PLAY };
    let resp = ui
        .put(btn_rect, egui::Button::new(RichText::new(icon).font(icons::fill(16.0))).corner_radius(17))
        .on_hover_text("เล่น / หยุด (Space)");
    if resp.clicked() {
        app.toggle_play(&ctx);
    }

    // อัตราส่วนภาพด้านขวา
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let before = app.project.aspect;
        egui::ComboBox::from_id_salt("aspect_combo").selected_text(format!("อัตราส่วน {}", app.project.aspect.label())).width(120.0).show_ui(
            ui,
            |ui| {
                for a in AspectRatio::ALL {
                    ui.selectable_value(&mut app.project.aspect, a, a.label());
                }
            },
        );
        if app.project.aspect != before {
            app.dirty = true;
            app.stop_playback();
        }
    });
}
