use eframe::egui::{self, Align2, FontId, RichText, Sense};

use crate::app::TadTorApp;
use crate::core::project::AspectRatio;
use crate::core::time::format_timecode;
use crate::core::timeline::TrackKind;
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
            let hint = if app.timeline.is_empty() {
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
    overlay_handle(app, ui, canvas);

    // ---- ปุ่มควบคุม ----
    controls(app, ui);
}

/// กรอบของภาพซ้อนที่เลือกอยู่ (ตรงหัวเล่น) ลากเพื่อย้ายตำแหน่ง
fn overlay_handle(app: &mut TadTorApp, ui: &mut egui::Ui, canvas: egui::Rect) {
    let Some(c) = app.selected_clip.and_then(|id| app.timeline.clip(id)) else { return };
    let t = app.playhead;
    if app.timeline.find(c.id).map(|f| f.0) != Some(TrackKind::Overlay) || t < c.start || t >= c.end() {
        return;
    }
    let Some(info) = app.library.get(&c.path).and_then(|it| it.info()) else { return };
    let (mut w, mut h) = (info.width.max(1) as f32, info.height.max(1) as f32);
    if c.fx.rotate % 2 == 1 {
        std::mem::swap(&mut w, &mut h);
    }
    // ขนาด/ตำแหน่งแบบเดียวกับ overlay_chain ใน ffmpeg/graph.rs
    let z = c.fx.zoom.clamp(0.1, 5.0);
    let s = (canvas.width() * z / w).min(canvas.height() * z / h);
    let size = egui::vec2(w * s, h * s);
    let room = canvas.size() - size;
    let at = canvas.min + egui::vec2(room.x * (1.0 + c.fx.pan_x) / 2.0, room.y * (1.0 + c.fx.pan_y) / 2.0);
    let rect = egui::Rect::from_min_size(at, size);
    let (id, pan) = (c.id, egui::vec2(c.fx.pan_x, c.fx.pan_y));

    let resp = ui
        .interact(rect.intersect(canvas), egui::Id::unique(("overlay_handle", id)), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::Grab);
    if resp.drag_started() {
        app.begin_drag();
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        let step = |delta: f32, room: f32, cur: f32| if room.abs() > 1.0 { (cur + delta * 2.0 / room).clamp(-1.0, 1.0) } else { cur };
        let (px, py) = (step(d.x, room.x, pan.x), step(d.y, room.y, pan.y));
        app.timeline.update_clip(id, |c| {
            c.fx.pan_x = px;
            c.fx.pan_y = py;
        });
        app.timeline_rev += 1;
        app.dirty = true;
    }
    if resp.drag_stopped() {
        app.end_drag();
    }

    // จุดจับ 4 มุมสำหรับลากปรับขนาดภาพซ้อนในเครื่องเล่น
    let corners = [
        (0usize, rect.left_top(), -1.0f32, -1.0f32, egui::CursorIcon::ResizeNwSe),
        (1, rect.right_top(), 1.0, -1.0, egui::CursorIcon::ResizeNeSw),
        (2, rect.left_bottom(), -1.0, 1.0, egui::CursorIcon::ResizeNeSw),
        (3, rect.right_bottom(), 1.0, 1.0, egui::CursorIcon::ResizeNwSe),
    ];
    let base_diag = canvas.size().length().max(1.0);
    for (ci, pos, sx, sy, cursor) in corners {
        let c_rect = egui::Rect::from_center_size(pos, egui::vec2(12.0, 12.0));
        let c_resp = ui
            .interact(c_rect, egui::Id::unique(("overlay_corner", id, ci)), Sense::drag())
            .on_hover_cursor(cursor);
        if c_resp.drag_started() {
            app.begin_drag();
        }
        if c_resp.dragged() {
            let d = c_resp.drag_delta();
            let delta_z = (d.x * sx + d.y * sy) * 1.4 / base_diag;
            let new_z = (z + delta_z).clamp(0.05, 2.0);
            app.timeline.update_clip(id, |c| c.fx.zoom = new_z);
            app.timeline_rev += 1;
            app.dirty = true;
        }
        if c_resp.drag_stopped() {
            app.end_drag();
        }
    }

    let p = ui.painter().with_clip_rect(canvas);
    p.rect_stroke(rect, 0, egui::Stroke::new(1.5, theme::ACCENT), egui::StrokeKind::Outside);
    for (_, pos, _, _, _) in corners {
        let dot = egui::Rect::from_center_size(pos, egui::vec2(7.0, 7.0));
        p.rect_filled(dot, 1, egui::Color32::WHITE);
        p.rect_stroke(dot, 1, egui::Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Outside);
    }
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
