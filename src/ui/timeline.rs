use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke};

use crate::app::{MediaDrag, TadTorApp};
use crate::core::media::{MediaKind, MediaState};
use crate::core::time::format_short;
use crate::core::timeline::{Clip, TrackKind};
use crate::ffmpeg::probe::WAVE_PER_SEC;
use crate::{icons, theme};

const HEADER_W: f32 = 96.0; // คอลัมน์ชื่อแทร็กด้านซ้าย
const RULER_H: f32 = 26.0;
const TRACK_GAP: f32 = 6.0;
const MIN_PX_PER_SEC: f32 = 4.0;
const MAX_PX_PER_SEC: f32 = 400.0;
/// ความกว้างที่จับขอบคลิปเพื่อตัดหัว/ท้าย
const HANDLE_W: f32 = 8.0;
/// ระยะดูดติดหัวเล่น/ขอบคลิปอื่น (พิกเซล)
const SNAP_PX: f32 = 8.0;

const CLIP_VIDEO: Color32 = Color32::from_rgb(0x1d, 0x3f, 0x47);
const CLIP_AUDIO: Color32 = Color32::from_rgb(0x1f, 0x45, 0x33);
const WAVE: Color32 = Color32::from_rgb(0x5f, 0xd3, 0x95);
const CLIP_TEXT: Color32 = Color32::from_rgb(0x4a, 0x2f, 0x6b);
const CLIP_SUB: Color32 = Color32::from_rgb(0x5c, 0x40, 0x1c);

fn lane_height(kind: TrackKind) -> f32 {
    match kind {
        TrackKind::Main => 64.0,
        TrackKind::Text | TrackKind::Subtitle => 28.0,
        TrackKind::Audio => 44.0,
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum DragMode {
    Move,
    Head,
    Tail,
}

/// สถานะการลากคลิป (เก็บในหน่วยความจำของ egui ระหว่างเฟรม)
#[derive(Clone, Copy, Debug)]
struct ClipDrag {
    id: u64,
    mode: DragMode,
    track: TrackKind,
    /// เวลาใต้เมาส์ตอนเริ่มลาก
    press_t: f64,
    orig_start: f64,
    orig_end: f64,
}

/// การ์ดไทม์ไลน์ (ด้านบนหรือด้านล่าง ดู `layout`)
pub fn content(app: &mut TadTorApp, ui: &mut egui::Ui) {
    toolbar(app, ui);
    tracks(app, ui);
}

fn fit_id() -> egui::Id {
    egui::Id::unique("timeline_fit")
}

fn toolbar(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    egui::Frame::new()
        .fill(theme::PANEL_HEADER)
        .corner_radius(egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                super::layout::grip(app, ui, super::layout::Card::Timeline);
                let tool = |ui: &mut egui::Ui, icon: &str, tip: &str, enabled: bool| {
                    ui.add_enabled(enabled, egui::Button::new(RichText::new(icon).size(15.0)).frame(false))
                        .on_hover_text(tip)
                        .on_disabled_hover_text(tip)
                        .clicked()
                };
                let has_clips = !app.timeline.is_empty();
                if tool(ui, icons::ARROW_U_UP_LEFT, "ย้อนกลับ (Ctrl+Z)", app.history.can_undo()) {
                    app.undo();
                }
                if tool(ui, icons::ARROW_U_UP_RIGHT, "ทำซ้ำ (Ctrl+Y)", app.history.can_redo()) {
                    app.redo();
                }
                ui.separator();
                if tool(ui, icons::SCISSORS, "แยกคลิปที่หัวเล่น (Ctrl+B)", has_clips) {
                    app.split_at_playhead(&ctx);
                }
                if tool(ui, icons::ARROW_LINE_LEFT, "ตัดหัวคลิปถึงหัวเล่น (Q)", has_clips) {
                    app.trim_head_to_playhead(&ctx);
                }
                if tool(ui, icons::ARROW_LINE_RIGHT, "ตัดท้ายคลิปตั้งแต่หัวเล่น (W)", has_clips)
                {
                    app.trim_tail_to_playhead(&ctx);
                }
                if tool(ui, icons::TRASH, "ลบคลิปที่เลือก (Delete)", app.selected_clip.is_some()) {
                    app.delete_selected();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(icons::MAGNIFYING_GLASS_PLUS).on_hover_text("ซูมเข้า (Ctrl + เลื่อนลูกกลิ้ง)").clicked()
                    {
                        app.px_per_sec = (app.px_per_sec * 1.25).min(MAX_PX_PER_SEC);
                    }
                    let mut z = app.px_per_sec.ln();
                    let slider = super::fader::Fader::new(&mut z, MIN_PX_PER_SEC.ln()..=MAX_PX_PER_SEC.ln()).hide_value().width(110.0);
                    if ui.add(slider).changed() {
                        app.px_per_sec = z.exp();
                    }
                    if ui.small_button(icons::MAGNIFYING_GLASS_MINUS).on_hover_text("ซูมออก").clicked() {
                        app.px_per_sec = (app.px_per_sec / 1.25).max(MIN_PX_PER_SEC);
                    }
                    if ui
                        .add_enabled(has_clips, egui::Button::new(format!("{} พอดีจอ", icons::ARROWS_OUT_LINE_HORIZONTAL)).small())
                        .on_hover_text("ซูมให้เห็นงานทั้งหมด")
                        .clicked()
                    {
                        ui.data_mut(|d| d.insert_temp(fit_id(), true));
                    }
                });
            });
        });
}

/// ระยะห่างของขีดใหญ่บนไม้บรรทัด (วินาที) ให้ห่างกันอย่างน้อย ~90px
fn major_step(px_per_sec: f32) -> f64 {
    const STEPS: [f64; 13] = [0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1200.0, 1800.0];
    STEPS.iter().copied().find(|s| *s as f32 * px_per_sec >= 90.0).unwrap_or(1800.0)
}

/// เวลาที่ใกล้ `t` ที่สุดใน `targets` ถ้าห่างไม่เกิน `tol`
fn snap(t: f64, targets: &[f64], tol: f64) -> Option<f64> {
    targets.iter().copied().filter(|s| (s - t).abs() <= tol).min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
}

fn tracks(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let size = ui.available_size();
    let (resp, painter) = ui.allocate_painter(size, Sense::click_and_drag());
    let full = resp.rect;

    // เก็บตำแหน่งเลื่อนแนวนอนไว้ในหน่วยความจำของ egui
    let scroll_id = egui::Id::unique("timeline_scroll_x");
    let mut scroll_x: f32 = ui.data(|d| d.get_temp(scroll_id).unwrap_or(0.0));

    let lanes_left = full.left() + HEADER_W;
    let lanes_w = (full.width() - HEADER_W).max(1.0);

    if ui.data_mut(|d| d.remove_temp::<bool>(fit_id())).is_some() {
        let d = app.timeline.duration().max(1.0) as f32;
        app.px_per_sec = (lanes_w * 0.92 / d).clamp(MIN_PX_PER_SEC, MAX_PX_PER_SEC);
        scroll_x = 0.0;
    }

    // ---- ซูม/เลื่อนด้วยเมาส์ ----
    if ui.rect_contains_pointer(full) {
        let (zoom, scroll, pointer) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
        if zoom != 1.0 {
            // ซูมโดยให้ตำแหน่งใต้เมาส์อยู่ที่เดิม
            let mouse_x = pointer.map(|p| p.x).unwrap_or(lanes_left).max(lanes_left);
            let t_at_mouse = (mouse_x - lanes_left + scroll_x) / app.px_per_sec;
            app.px_per_sec = (app.px_per_sec * zoom).clamp(MIN_PX_PER_SEC, MAX_PX_PER_SEC);
            scroll_x = t_at_mouse * app.px_per_sec - (mouse_x - lanes_left);
        } else {
            // ลูกกลิ้งปกติ = เลื่อนซ้าย/ขวา
            scroll_x -= scroll.x + scroll.y;
        }
    }

    // หัวเล่นวิ่งพ้นจอตอนกำลังเล่น → เลื่อนตาม
    let head_px = app.playhead as f32 * app.px_per_sec;
    if app.playing && head_px - scroll_x > lanes_w * 0.9 {
        scroll_x = head_px - lanes_w * 0.1;
    }

    let content_w = (app.timeline_end() as f32 * app.px_per_sec + lanes_w * 0.5).max(lanes_w);
    scroll_x = scroll_x.clamp(0.0, (content_w - lanes_w).max(0.0));
    ui.data_mut(|d| d.insert_temp(scroll_id, scroll_x));

    let pps = app.px_per_sec;
    let time_to_x = |t: f64| lanes_left + t as f32 * pps - scroll_x;
    let x_to_time = |x: f32| ((x - lanes_left + scroll_x) / pps).max(0.0) as f64;

    // ---- พื้นที่แต่ละส่วน ----
    let ruler = Rect::from_min_max(Pos2::new(lanes_left, full.top()), Pos2::new(full.right(), full.top() + RULER_H));
    let mut y = ruler.bottom() + 10.0;
    let lanes: Vec<(TrackKind, Rect)> = TrackKind::ALL
        .into_iter()
        .map(|k| {
            let r = Rect::from_min_max(Pos2::new(lanes_left, y), Pos2::new(full.right() - 8.0, y + lane_height(k)));
            y = r.bottom() + TRACK_GAP;
            (k, r)
        })
        .collect();
    let lane_of = |k: TrackKind| lanes.iter().find(|l| l.0 == k).unwrap().1;
    let main_lane = lane_of(TrackKind::Main);
    let audio_lane = lane_of(TrackKind::Audio);
    let lanes_clip = Rect::from_min_max(Pos2::new(lanes_left, full.top()), full.max);
    let lp = painter.with_clip_rect(lanes_clip);

    // ---- คลิก/ลากพื้นที่ว่างหรือไม้บรรทัด = ย้ายหัวเล่น ----
    if resp.drag_started() && app.playing {
        app.stop_playback();
    }
    if let Some(pos) = resp.interact_pointer_pos() {
        if (resp.clicked() || resp.dragged()) && pos.x >= lanes_left {
            app.seek(x_to_time(pos.x));
        }
    }
    if resp.clicked() {
        app.selected_clip = None;
    }

    // ---- พื้นหลัง ----
    painter.rect_filled(full, 0, theme::PANEL);

    // ---- ไม้บรรทัดเวลา ----
    let major = major_step(pps);
    let minor = major / 5.0;
    let t_start = x_to_time(lanes_left);
    let t_end = x_to_time(full.right());
    let mut t = (t_start / minor).floor() * minor;
    while t <= t_end + minor {
        let x = time_to_x(t);
        let is_major = ((t / major).round() * major - t).abs() < 1e-6;
        let h = if is_major { 10.0 } else { 5.0 };
        lp.line_segment(
            [Pos2::new(x, ruler.bottom() - h), Pos2::new(x, ruler.bottom())],
            Stroke::new(1.0, if is_major { theme::TEXT_DIM } else { theme::BORDER }),
        );
        if is_major {
            let label = if major < 1.0 { format!("{}.{}", format_short(t.floor()), ((t.fract()) * 10.0).round()) } else { format_short(t) };
            lp.text(Pos2::new(x + 4.0, ruler.top() + 8.0), Align2::LEFT_CENTER, label, FontId::proportional(11.5), theme::TEXT_DIM);
        }
        t += minor;
    }
    painter
        .line_segment([Pos2::new(full.left(), ruler.bottom()), Pos2::new(full.right(), ruler.bottom())], Stroke::new(1.0, theme::BORDER));

    // ---- หัวแทร็ก (ชื่อ + ปุ่มปิดเสียง/ซ่อน) ----
    for &(kind, lane) in &lanes {
        if track_header(app, ui, &painter, full.left(), lane, kind) {
            app.edit(|tl| match kind {
                TrackKind::Main => tl.main_muted = !tl.main_muted,
                TrackKind::Audio => tl.audio_muted = !tl.audio_muted,
                TrackKind::Text => tl.text_hidden = !tl.text_hidden,
                TrackKind::Subtitle => tl.subtitles_hidden = !tl.subtitles_hidden,
            });
        }
        lp.rect_filled(lane, 6, theme::TRACK_BG);
    }
    if app.timeline.main.is_empty() {
        super::dashed_rect(&lp, main_lane.shrink(1.0), theme::BORDER);
        lp.text(
            Pos2::new(main_lane.left() + 16.0, main_lane.center().y),
            Align2::LEFT_CENTER,
            "ลากวิดีโอหรือรูปจากคลังสื่อมาวางที่นี่ (หรือกด + บนการ์ดไฟล์)",
            FontId::proportional(12.5),
            theme::TEXT_DIM,
        );
    }
    for (kind, hint) in [
        (TrackKind::Text, "เพิ่มข้อความจากแท็บ \"ข้อความ\" ด้านซ้าย"),
        (TrackKind::Subtitle, "ซับไตเติล: สร้างอัตโนมัติหรือนำเข้า .srt จากแท็บ \"ข้อความ\""),
        (TrackKind::Audio, "ไฟล์เพลง/เสียงจะมาอยู่แทร็กนี้"),
    ] {
        if app.timeline.track(kind).is_empty() {
            let lane = lane_of(kind);
            lp.text(
                Pos2::new(lane.left() + 16.0, lane.center().y),
                Align2::LEFT_CENTER,
                hint,
                FontId::proportional(12.0),
                theme::TEXT_DIM.gamma_multiply(0.7),
            );
        }
    }

    // ---- คลิป ----
    let drag_id = egui::Id::unique("timeline_clip_drag");
    let mut drag: Option<ClipDrag> = ui.data(|d| d.get_temp(drag_id));
    let pointer = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));
    let pointer_t = pointer.map(|p| x_to_time(p.x));
    let snap_tol = (SNAP_PX / pps) as f64;
    let mut main_ghost: Option<(Clip, f32)> = None;

    for &(kind, lane) in &lanes {
        let clips = app.timeline.track(kind).clone();
        for c in &clips {
            let rect = Rect::from_min_max(
                Pos2::new(time_to_x(c.start) + 1.0, lane.top()),
                Pos2::new((time_to_x(c.end()) - 1.0).max(time_to_x(c.start) + 3.0), lane.bottom()),
            );
            let vis = rect.intersect(lanes_clip);
            let this_drag = drag.filter(|d| d.id == c.id);
            if !vis.is_positive() && this_drag.is_none() {
                continue;
            }
            let hit = if vis.is_positive() { vis } else { rect };
            let body = ui.interact(hit, egui::Id::unique(("clip", c.id)), Sense::click_and_drag());
            let hw = HANDLE_W.min(rect.width() / 3.0);
            let head_r = Rect::from_min_max(rect.min, Pos2::new(rect.left() + hw, rect.bottom())).intersect(lanes_clip);
            let tail_r = Rect::from_min_max(Pos2::new(rect.right() - hw, rect.top()), rect.max).intersect(lanes_clip);
            let head = ui
                .interact(head_r, egui::Id::unique(("clip_head", c.id)), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            let tail = ui
                .interact(tail_r, egui::Id::unique(("clip_tail", c.id)), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);

            // ---- เริ่มลาก ----
            let started =
                [(DragMode::Head, &head), (DragMode::Tail, &tail), (DragMode::Move, &body)].into_iter().find(|(_, r)| r.drag_started());
            if let (Some((mode, _)), Some(pt)) = (started, pointer_t) {
                app.begin_drag();
                app.selected_clip = Some(c.id);
                app.selected_media = None;
                drag = Some(ClipDrag { id: c.id, mode, track: kind, press_t: pt, orig_start: c.start, orig_end: c.end() });
            }
            if body.clicked() {
                app.selected_clip = Some(c.id);
                app.selected_media = None;
            }

            // ---- ระหว่างลาก ----
            let active = drag.filter(|d| d.id == c.id);
            let dragging = head.dragged() || tail.dragged() || body.dragged();
            if let (Some(d), Some(pt), true) = (active, pointer_t, dragging) {
                if let Some(origin) = app.drag_origin.clone() {
                    let targets: Vec<f64> = std::iter::once(0.0)
                        .chain(std::iter::once(app.playhead))
                        .chain(origin.all_clips().filter(|o| o.id != d.id).flat_map(|o| [o.start, o.end()]))
                        .collect();
                    let mut delta = pt - d.press_t;
                    let mut tl = origin.clone();
                    match d.mode {
                        DragMode::Head => {
                            if d.track != TrackKind::Main {
                                if let Some(s) = snap(d.orig_start + delta, &targets, snap_tol) {
                                    delta = s - d.orig_start;
                                }
                            }
                            tl.trim_head(d.id, delta);
                        }
                        DragMode::Tail => {
                            if let Some(s) = snap(d.orig_end + delta, &targets, snap_tol) {
                                delta = s - d.orig_end;
                            }
                            tl.trim_tail(d.id, delta);
                        }
                        DragMode::Move if d.track != TrackKind::Main => {
                            let len = d.orig_end - d.orig_start;
                            let mut s = d.orig_start + delta;
                            if let Some(v) = snap(s, &targets, snap_tol) {
                                s = v;
                            } else if let Some(v) = snap(s + len, &targets, snap_tol) {
                                s = v - len;
                            }
                            tl.move_free(d.id, s);
                        }
                        DragMode::Move => {
                            // แทร็กหลัก: ยังไม่ย้ายจริง แค่วาดคลิปลอยตามเมาส์ ปล่อยแล้วค่อยแทรก
                            let x = time_to_x(d.orig_start + delta);
                            main_ghost = Some((c.clone(), x));
                        }
                    }
                    if tl != app.timeline {
                        app.timeline = tl;
                        app.timeline_rev += 1;
                    }
                }
            }

            // ---- ปล่อยเมาส์ ----
            let stopped = head.drag_stopped() || tail.drag_stopped() || body.drag_stopped();
            if let (Some(d), true) = (active, stopped) {
                if d.mode == DragMode::Move && d.track == TrackKind::Main {
                    if let (Some(p), Some(origin)) = (pointer, app.drag_origin.as_ref()) {
                        let to = origin.main.iter().filter(|o| time_to_x((o.start + o.end()) / 2.0) < p.x).count();
                        app.timeline.move_main(d.id, to);
                    }
                }
                app.end_drag();
                drag = None;
            }

            let selected = app.selected_clip == Some(c.id);
            let ghosted = main_ghost.as_ref().is_some_and(|(g, _)| g.id == c.id);
            if !ghosted {
                draw_clip(app, &lp, rect, c, selected, kind, pps);
            } else {
                lp.rect_filled(rect, 5, theme::SURFACE.gamma_multiply(0.5));
            }
        }
    }

    if main_ghost.is_none() {
        transition_markers(app, ui, &lp, main_lane, lanes_clip, &time_to_x);
    }

    // ลากคลิปหลักอยู่: วาดคลิปลอย + เส้นบอกตำแหน่งที่จะแทรก
    if let (Some((c, x)), Some(p)) = (&main_ghost, pointer) {
        let w = (c.dur as f32 * pps).max(3.0);
        let r = Rect::from_min_size(Pos2::new(*x, main_lane.top() - 4.0), egui::vec2(w, main_lane.height()));
        let marker = insertion_x(app, p.x, &time_to_x);
        draw_marker(&lp, marker, main_lane);
        draw_clip(app, &lp, r, c, true, TrackKind::Main, pps);
        lp.rect_filled(r, 5, Color32::from_black_alpha(60));
    }

    // ---- ลากไฟล์จากคลังสื่อมาวาง ----
    drop_from_library(app, ui, &ctx, &lp, full, main_lane, audio_lane, pointer, &time_to_x, &x_to_time);

    // ---- หัวเล่น ----
    let hx = time_to_x(app.playhead);
    if hx >= lanes_left - 1.0 && hx <= full.right() {
        lp.line_segment([Pos2::new(hx, ruler.top() + 4.0), Pos2::new(hx, full.bottom())], Stroke::new(1.5, theme::PLAYHEAD));
        // หัวสามเหลี่ยมด้านบน
        let top = ruler.top() + 2.0;
        lp.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(hx - 6.0, top),
                Pos2::new(hx + 6.0, top),
                Pos2::new(hx + 6.0, top + 8.0),
                Pos2::new(hx, top + 14.0),
                Pos2::new(hx - 6.0, top + 8.0),
            ],
            theme::PLAYHEAD,
            Stroke::NONE,
        ));
    }

    if drag.is_none() && ui.input(|i| !i.pointer.any_down()) {
        // ป้องกันค้าง: ถ้าปล่อยเมาส์ไปแล้วแต่ยังมีสถานะลากเหลืออยู่
        if app.drag_origin.is_some() {
            app.end_drag();
        }
    }
    ui.data_mut(|d| match drag {
        Some(v) => {
            d.insert_temp(drag_id, v);
        }
        None => d.remove::<ClipDrag>(drag_id),
    });
}

/// ตำแหน่ง x ของรอยต่อในแทร็กหลักที่ใกล้เมาส์ที่สุด (ที่ที่คลิปจะถูกแทรก)
fn insertion_x(app: &TadTorApp, px: f32, time_to_x: &dyn Fn(f64) -> f32) -> f32 {
    let tl = app.drag_origin.as_ref().unwrap_or(&app.timeline);
    match tl.main.iter().find(|c| time_to_x((c.start + c.end()) / 2.0) >= px) {
        Some(c) => time_to_x(c.start),
        None => time_to_x(tl.main_end()),
    }
}

fn draw_marker(p: &egui::Painter, x: f32, lane: Rect) {
    p.line_segment([Pos2::new(x, lane.top() - 6.0), Pos2::new(x, lane.bottom() + 6.0)], Stroke::new(3.0, theme::ACCENT));
}

#[allow(clippy::too_many_arguments)]
fn drop_from_library(
    app: &mut TadTorApp,
    ui: &egui::Ui,
    ctx: &egui::Context,
    lp: &egui::Painter,
    full: Rect,
    main_lane: Rect,
    audio_lane: Rect,
    pointer: Option<Pos2>,
    time_to_x: &dyn Fn(f64) -> f32,
    x_to_time: &dyn Fn(f32) -> f64,
) {
    let Some(payload) = egui::DragAndDrop::payload::<MediaDrag>(ctx) else { return };
    let Some(p) = pointer.filter(|p| full.contains(*p)) else { return };
    let Some(item) = app.library.get(&payload.path) else { return };
    let (kind, info) = (item.kind, item.info().cloned());

    let mut main_index = None;
    let mut audio_at = None;
    if kind.is_visual() {
        let index = app.timeline.main.iter().filter(|c| time_to_x((c.start + c.end()) / 2.0) < p.x).count();
        draw_marker(lp, insertion_x(app, p.x, time_to_x), main_lane);
        main_index = Some(index);
    } else {
        let t = x_to_time(p.x);
        let dur = info.and_then(|i| i.duration).unwrap_or(5.0);
        let r = Rect::from_min_size(
            Pos2::new(time_to_x(t), audio_lane.top()),
            egui::vec2((dur as f32 * app.px_per_sec).max(3.0), audio_lane.height()),
        );
        lp.rect_filled(r, 5, theme::ACCENT_DARK.gamma_multiply(0.6));
        lp.rect_stroke(r, 5, Stroke::new(1.5, theme::ACCENT), egui::StrokeKind::Inside);
        audio_at = Some(t);
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);

    if ui.input(|i| i.pointer.any_released()) {
        egui::DragAndDrop::clear_payload(ctx);
        let path = payload.path.clone();
        app.add_to_timeline(ctx, &path, main_index, audio_at);
    }
}

/// วาดหัวแทร็ก คืน true ถ้ากดปุ่มปิดเสียง/ซ่อนแทร็ก
fn track_header(app: &TadTorApp, ui: &egui::Ui, painter: &egui::Painter, left: f32, lane: Rect, kind: TrackKind) -> bool {
    let tl = &app.timeline;
    let (icon, off, (on_icon, off_icon), tips) = match kind {
        TrackKind::Main => (icons::FILM_STRIP, tl.main_muted, (icons::SPEAKER_HIGH, icons::SPEAKER_SLASH), ("ปิดเสียงแทร็กนี้", "เปิดเสียงแทร็กนี้")),
        TrackKind::Audio => {
            (icons::MUSIC_NOTES, tl.audio_muted, (icons::SPEAKER_HIGH, icons::SPEAKER_SLASH), ("ปิดเสียงแทร็กนี้", "เปิดเสียงแทร็กนี้"))
        }
        TrackKind::Text => (icons::TEXT_T, tl.text_hidden, (icons::EYE, icons::EYE_SLASH), ("ซ่อนข้อความ", "แสดงข้อความ")),
        TrackKind::Subtitle => (icons::SUBTITLES, tl.subtitles_hidden, (icons::EYE, icons::EYE_SLASH), ("ซ่อนซับไตเติล", "แสดงซับไตเติล")),
    };
    let r = Rect::from_min_max(Pos2::new(left + 8.0, lane.top()), Pos2::new(left + HEADER_W - 4.0, lane.bottom()));
    let tall = lane.height() > 40.0;
    let name_y = if tall { r.center().y - 9.0 } else { r.center().y };
    painter.text(Pos2::new(r.left() + 4.0, name_y), Align2::LEFT_CENTER, icon, FontId::proportional(14.0), theme::TEXT_DIM);
    painter.text(Pos2::new(r.left() + 24.0, name_y), Align2::LEFT_CENTER, kind.label(), FontId::proportional(12.0), theme::TEXT_DIM);

    let btn_c = if tall { Pos2::new(r.left() + 13.0, r.center().y + 12.0) } else { Pos2::new(r.right() - 10.0, r.center().y) };
    let btn = Rect::from_center_size(btn_c, egui::vec2(22.0, 20.0));
    let resp = ui.interact(btn, egui::Id::unique(("track_toggle", kind)), Sense::click()).on_hover_text(if off { tips.1 } else { tips.0 });
    if resp.hovered() {
        painter.rect_filled(btn, 4, theme::SURFACE);
    }
    let color = if off { theme::ERROR } else { theme::TEXT_DIM };
    painter.text(btn.center(), Align2::CENTER_CENTER, if off { off_icon } else { on_icon }, FontId::proportional(13.0), color);
    resp.clicked()
}

/// ปุ่มรูปเพชรตรงรอยต่อคลิปหลัก กดแล้วไปแท็บทรานซิชัน (มีทรานซิชันแล้วจะเป็นสีฟ้า)
fn transition_markers(app: &mut TadTorApp, ui: &egui::Ui, lp: &egui::Painter, lane: Rect, clip: Rect, time_to_x: &dyn Fn(f64) -> f32) {
    let mut open = None;
    for i in 0..app.timeline.main.len().saturating_sub(1) {
        let trans = app.timeline.transition_after(i);
        let next = &app.timeline.main[i + 1];
        let x = match trans {
            Some((_, d)) => time_to_x(next.start + d / 2.0),
            None => time_to_x(next.start),
        };
        if x < clip.left() || x > clip.right() {
            continue;
        }
        if let Some((_, d)) = trans {
            let r = Rect::from_min_max(Pos2::new(time_to_x(next.start), lane.top()), Pos2::new(time_to_x(next.start + d), lane.bottom()));
            lp.rect_filled(r, 2, theme::ACCENT.gamma_multiply(0.25));
        }
        let c = Pos2::new(x, lane.bottom() - 10.0);
        let hit = Rect::from_center_size(c, egui::vec2(18.0, 18.0));
        let resp = ui.interact(hit, egui::Id::unique(("transition", app.timeline.main[i].id)), Sense::click()).on_hover_text(match trans {
            Some((k, d)) => format!("ทรานซิชัน: {} ({d:.1} วิ) — คลิกเพื่อแก้", k.label()),
            None => "เพิ่มทรานซิชัน".to_owned(),
        });
        let fill = match (trans.is_some(), resp.hovered()) {
            (true, _) => theme::ACCENT,
            (false, true) => theme::TEXT,
            (false, false) => Color32::from_gray(150),
        };
        let s = 6.0;
        lp.add(egui::Shape::convex_polygon(
            vec![c + egui::vec2(0.0, -s), c + egui::vec2(s, 0.0), c + egui::vec2(0.0, s), c + egui::vec2(-s, 0.0)],
            fill,
            Stroke::new(1.0, Color32::from_black_alpha(180)),
        ));
        if resp.clicked() {
            open = Some(app.timeline.main[i].id);
        }
    }
    if let Some(id) = open {
        app.selected_clip = Some(id);
        app.selected_media = None;
        app.left_tab = crate::app::LeftTab::Transitions;
    }
}

fn draw_clip(app: &TadTorApp, lp: &egui::Painter, rect: Rect, c: &Clip, selected: bool, kind: TrackKind, pps: f32) {
    let clip_p = lp.with_clip_rect(rect.intersect(lp.clip_rect()));
    let vis = clip_p.clip_rect();
    let muted = match kind {
        TrackKind::Main => app.timeline.main_muted && c.kind == MediaKind::Video,
        TrackKind::Audio => app.timeline.audio_muted,
        TrackKind::Text => app.timeline.text_hidden,
        TrackKind::Subtitle => app.timeline.subtitles_hidden,
    };
    let missing = app.library.get(&c.path).is_some_and(|it| matches!(it.state, MediaState::Failed(_)));

    match kind {
        TrackKind::Main => {
            clip_p.rect_filled(rect, 5, CLIP_VIDEO);
            if let Some(tex) = app.thumbs.get(&c.path) {
                let [tw, th] = tex.size();
                let tile_h = rect.height();
                let tile_w = (tile_h * tw as f32 / th.max(1) as f32).max(8.0);
                let first = ((vis.left() - rect.left()) / tile_w).floor().max(0.0);
                let mut x = rect.left() + first * tile_w;
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                while x < vis.right() {
                    let tile = Rect::from_min_size(Pos2::new(x, rect.top()), egui::vec2(tile_w, tile_h));
                    clip_p.image(tex.id(), tile, uv, Color32::from_gray(200));
                    x += tile_w;
                }
            }
        }
        TrackKind::Text | TrackKind::Subtitle => {
            let fill = if kind == TrackKind::Text { CLIP_TEXT } else { CLIP_SUB };
            clip_p.rect_filled(rect, 5, fill);
        }
        TrackKind::Audio => {
            clip_p.rect_filled(rect, 5, CLIP_AUDIO);
            if let Some(peaks) = app.waveforms.get(&c.path) {
                let mid = rect.center().y + 6.0;
                let half = (rect.height() - 20.0) / 2.0;
                let step = 2.0;
                let mut x = vis.left().max(rect.left());
                while x < vis.right().min(rect.right()) {
                    let t0 = c.src_in + ((x - rect.left()) / pps) as f64;
                    let i0 = (t0 * WAVE_PER_SEC as f64) as usize;
                    let i1 = (((t0 + (step / pps) as f64) * WAVE_PER_SEC as f64).ceil() as usize).max(i0 + 1);
                    let peak = peaks.get(i0..i1.min(peaks.len())).map(|s| s.iter().copied().fold(0.0, f32::max)).unwrap_or(0.0);
                    let h = (peak * half).max(0.5);
                    clip_p.line_segment([Pos2::new(x, mid - h), Pos2::new(x, mid + h)], Stroke::new(1.0, WAVE));
                    x += step;
                }
            }
        }
    }

    // แถบชื่อคลิปด้านบน (คลิปข้อความเตี้ย ใช้ทั้งคลิปเป็นป้าย)
    let label_r = if kind.is_text() { rect } else { Rect::from_min_size(rect.min, egui::vec2(rect.width(), 16.0)) };
    if !kind.is_text() {
        clip_p.rect_filled(label_r, egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, Color32::from_black_alpha(140));
    }
    let mut badges = String::new();
    if c.fx.filter != crate::core::fx::FilterPreset::None || c.fx.has_adjust() {
        badges.push_str(icons::PALETTE);
    }
    if c.fx.effect != crate::core::fx::Effect::None {
        badges.push_str(icons::SPARKLE);
    }
    if c.fx.voice != crate::core::fx::Voice::None || (c.fx.volume - 1.0).abs() > 1e-3 || c.fx.fade_in > 0.0 || c.fx.fade_out > 0.0 {
        badges.push_str(icons::MICROPHONE);
    }
    let text = if kind.is_text() { c.title().to_owned() } else { format!("{badges} {}  {:.1}s", c.title(), c.dur).trim_start().to_owned() };
    clip_p.text(
        Pos2::new(rect.left().max(vis.left()) + 5.0, label_r.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.5),
        theme::TEXT,
    );

    if muted {
        clip_p.rect_filled(rect, 5, Color32::from_black_alpha(110));
    }
    if missing {
        clip_p.rect_filled(rect, 5, theme::ERROR.gamma_multiply(0.35));
        clip_p.text(rect.center(), Align2::CENTER_CENTER, "ไม่พบไฟล์", FontId::proportional(12.0), Color32::WHITE);
    }

    if selected {
        lp.rect_stroke(rect, 5, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
        // ที่จับขอบสำหรับตัดหัว/ท้าย
        for x in [rect.left() + 3.0, rect.right() - 3.0] {
            let h = Rect::from_center_size(Pos2::new(x, rect.center().y), egui::vec2(3.0, rect.height() * 0.45));
            lp.rect_filled(h, 1, Color32::WHITE);
        }
    } else {
        lp.rect_stroke(rect, 5, Stroke::new(1.0, Color32::from_black_alpha(160)), egui::StrokeKind::Inside);
    }
}
