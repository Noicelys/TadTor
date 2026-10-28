//! แท็บเครื่องมือในแผงซ้าย: ข้อความ, เสียง, เอฟเฟกต์, ทรานซิชัน, ฟิลเตอร์, ปรับแต่ง
//! ส่วนใหญ่ใช้กับ "คลิปที่เลือก" ในไทม์ไลน์

use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense};

use super::fader::Fader;
use crate::app::{LeftTab, TadTorApp, Want};
use crate::core::fx::{
    ClipFx, Effect, FilterPreset, Motion, Pose, Rgba, TextLoop, TextMotion, TextStyle, Transition, TransitionKind, Voice,
    TRANSITION_DEFAULT,
};
use crate::core::timeline::TrackKind;
use crate::{icons, theme};

pub fn show(app: &mut TadTorApp, ui: &mut egui::Ui, tab: LeftTab) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| match tab {
            LeftTab::Text => text_tab(app, ui),
            LeftTab::Subtitles => super::subtitle_dialog::tab(app, ui),
            LeftTab::Audio => audio_tab(app, ui),
            LeftTab::Effects => effects_tab(app, ui),
            LeftTab::Transitions => transitions_tab(app, ui),
            LeftTab::Filters => filters_tab(app, ui),
            LeftTab::Adjust => adjust_tab(app, ui),
            LeftTab::Media => {}
        });
    });
}

pub(super) fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong());
    ui.add_space(2.0);
}

pub(super) fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::TEXT_DIM).size(13.0));
}

/// การ์ดตัวเลือกแบบตาราง คืนตัวที่ถูกคลิก
fn grid<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    items: &[T],
    current: Option<T>,
    label: impl Fn(T) -> String,
    swatch: impl Fn(T) -> Color32,
) -> Option<T> {
    card_grid(ui, items, current, label, |p, sw, item, _| {
        p.rect_filled(sw, 4, swatch(item));
    })
}

/// เหมือน `grid` แต่วาดภาพในการ์ดเอง `paint` ได้เวลา (วินาที) ตอนเมาส์ชี้การ์ดนั้น ไว้ทำภาพขยับ
pub(super) fn card_grid<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    items: &[T],
    current: Option<T>,
    label: impl Fn(T) -> String,
    paint: impl Fn(&egui::Painter, egui::Rect, T, Option<f32>),
) -> Option<T> {
    let gap = 8.0;
    let w = ((ui.available_width() - gap * 2.0) / 3.0).floor().max(60.0);
    let h = 58.0;
    let mut picked = None;
    for row in items.chunks(3) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &item in row {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), Sense::click());
                let p = ui.painter();
                let selected = current == Some(item);
                p.rect_filled(rect, 6, if resp.hovered() { theme::SURFACE_HOVER } else { theme::SURFACE });
                let sw = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 6.0), egui::vec2(w - 12.0, h - 30.0));
                let time = resp.hovered().then(|| {
                    ui.ctx().request_repaint();
                    ui.input(|i| i.time) as f32
                });
                paint(&p.with_clip_rect(sw), sw, item, time);
                p.text(
                    egui::pos2(rect.center().x, rect.bottom() - 12.0),
                    Align2::CENTER_CENTER,
                    label(item),
                    FontId::proportional(12.5),
                    if selected { theme::ACCENT } else { theme::TEXT },
                );
                if selected {
                    p.rect_stroke(rect, 6, egui::Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
                }
                if resp.clicked() {
                    picked = Some(item);
                }
            }
        });
        ui.add_space(gap);
    }
    picked
}

/// ชื่อคลิปที่เลือก หรือคำแนะนำให้เลือกคลิป
fn target_line(app: &TadTorApp, ui: &mut egui::Ui, want: Want) -> Option<u64> {
    let c = app.selected_clip.and_then(|id| app.timeline.clip(id));
    let track = c.and_then(|c| app.timeline.find(c.id)).map(|f| f.0);
    let ok = match (c, want) {
        (None, _) => false,
        (Some(_), Want::Visual) => track.is_some_and(TrackKind::is_visual),
        (Some(_), Want::Main) => track == Some(TrackKind::Main),
        (Some(c), Want::Sound) => c.has_audio,
        (Some(_), Want::Text) => track.is_some_and(TrackKind::is_text),
    };
    match c {
        Some(c) if ok => {
            ui.label(RichText::new(format!("ใช้กับ: {}", super::ellipsize(c.title(), 34))).color(theme::ACCENT).size(13.0));
            Some(c.id)
        }
        _ => {
            let msg = match want {
                Want::Visual => "คลิกเลือกคลิปวิดีโอ/รูปในไทม์ไลน์ แล้วเลือกด้านล่าง",
                Want::Main => "คลิกเลือกคลิปในแทร็กหลัก แล้วเลือกด้านล่าง",
                Want::Sound => "คลิกเลือกคลิปที่มีเสียงในไทม์ไลน์ แล้วเลือกด้านล่าง",
                Want::Text => "คลิกเลือกข้อความในไทม์ไลน์เพื่อเปลี่ยนสไตล์",
            };
            hint(ui, msg);
            None
        }
    }
}

// ---------------- ข้อความ + ซับไตเติล ----------------

fn text_tab(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let add =
        egui::Button::new(RichText::new(format!("{}  เพิ่มข้อความ", icons::PLUS)).strong()).min_size(egui::vec2(ui.available_width(), 34.0));
    if ui.add(add).on_hover_text("เพิ่มข้อความที่หัวเล่น (T)").clicked() {
        app.add_text(TextStyle::default());
    }

    section(ui, "สไตล์สำเร็จรูป");
    let target = target_line(app, ui, Want::Text);
    hint(ui, "ชี้เมาส์ที่การ์ดเพื่อดูตัวอย่างแบบขยับ");
    let presets = TextStyle::presets();
    let idx: Vec<usize> = (0..presets.len()).collect();
    let picked = card_grid(ui, &idx, None, |i| presets[i].0.to_owned(), |p, rect, i, time| paint_text_sample(p, rect, &presets[i].1, time));
    if let Some(i) = picked {
        let style = presets[i].1.clone();
        match target {
            // เปลี่ยนสไตล์ข้อความที่เลือก (คงข้อความและฟอนต์เดิมไว้)
            Some(id) => app.edit(|tl| {
                tl.update_clip(id, |c| {
                    let (content, font) = c.text.as_ref().map(|t| (t.content.clone(), t.font.clone())).unwrap_or_default();
                    c.text = Some(TextStyle { content, font, ..style });
                })
            }),
            None => app.add_text(style),
        }
    }

    motion_grids(app, ui, target);

    section(ui, &format!("{}  ซับไตเติล", icons::SUBTITLES));
    let full = egui::vec2(ui.available_width(), 30.0);
    if ui.add(egui::Button::new(format!("{}  ไปแท็บซับไตเติล (สร้างซับอัตโนมัติ / แก้ซับ)", icons::SUBTITLES)).min_size(full)).clicked()
    {
        app.left_tab = LeftTab::Subtitles;
    }
}

/// ภาพเคลื่อนไหวของข้อความ: เลือกจากการ์ดที่ขยับให้ดูเป็นตัวอย่าง
fn motion_grids(app: &mut TadTorApp, ui: &mut egui::Ui, target: Option<u64>) {
    let cur = target.and_then(|id| app.timeline.clip(id)).and_then(|c| c.text.as_ref()).map(|t| t.motion.clone());
    let base = TextStyle::default();
    // ค่าใหม่หลังกดการ์ด (ยังไม่ได้เลือกข้อความ = เริ่มจากไม่มีภาพเคลื่อนไหว)
    let mut next = cur.clone().unwrap_or_default();
    let before = next.clone();

    section(ui, &format!("{}  ภาพเคลื่อนไหว: ตอนเข้า", icons::SPARKLE));
    let sample = |m: Motion| TextStyle { motion: m, ..base.clone() };
    if let Some(k) = card_grid(
        ui,
        TextMotion::ALL,
        cur.as_ref().map(|m| m.anim_in),
        |k| k.label().to_owned(),
        |p, r, k, t| paint_text_sample(p, r, &sample(Motion { anim_in: k, ..Motion::default() }), t),
    ) {
        next.anim_in = k;
    }

    section(ui, &format!("{}  ภาพเคลื่อนไหว: ตอนออก", icons::SPARKLE));
    if let Some(k) = card_grid(
        ui,
        TextMotion::ALL,
        cur.as_ref().map(|m| m.anim_out),
        |k| k.label().to_owned(),
        |p, r, k, t| paint_text_sample(p, r, &sample(Motion { anim_out: k, ..Motion::default() }), t),
    ) {
        next.anim_out = k;
    }

    section(ui, &format!("{}  ภาพเคลื่อนไหว: วนซ้ำ", icons::SPARKLE));
    if let Some(k) = card_grid(
        ui,
        TextLoop::ALL,
        cur.as_ref().map(|m| m.looping),
        |k| k.label().to_owned(),
        |p, r, k, t| paint_text_sample(p, r, &sample(Motion { looping: k, ..Motion::default() }), t),
    ) {
        next.looping = k;
    }

    if next != before {
        match target {
            Some(id) => app.edit(|tl| tl.update_clip(id, |c| c.text.as_mut().unwrap().motion = next)),
            // ยังไม่ได้เลือกข้อความ: เพิ่มข้อความใหม่ที่มีภาพเคลื่อนไหวนี้
            None => app.add_text(TextStyle { motion: next, ..TextStyle::default() }),
        }
    }
    hint(ui, "ปรับความยาว/ความเร็วได้ที่แผงขวาหลังเลือกข้อความ");
}

/// วาดตัวอย่างข้อความตามสไตล์ลงในการ์ด `time` = เวลาตอนเมาส์ชี้ (None = นิ่งในท่าปกติ)
/// ใช้สูตรภาพเคลื่อนไหวเดียวกับตอนเรนเดอร์ (`Motion::pose_at`)
pub(super) fn paint_text_sample(p: &egui::Painter, rect: egui::Rect, style: &TextStyle, time: Option<f32>) {
    // พื้นแทนภาพวิดีโอ
    p.rect_filled(rect, 4, Color32::from_rgb(0x3a, 0x44, 0x52));
    let m = &style.motion;
    // มีท่าเข้า/ออก: เล่นเป็นคลิปยาว 1.6 วิ เว้นว่าง 0.5 วิ แล้ววนใหม่
    const CLIP: f32 = 1.6;
    let in_out = m.anim_in != TextMotion::None || m.anim_out != TextMotion::None;
    let pose = match time {
        None => Pose { dx: 0.0, dy: 0.0, scale: 1.0, alpha: 1.0 },
        Some(t) if in_out => {
            let t = t % (CLIP + 0.5);
            if t > CLIP {
                return;
            }
            m.pose_at(t, CLIP)
        }
        Some(t) => m.pose_at(t, f32::MAX),
    };
    let size = rect.height() * 0.42 * (style.size / 7.0).clamp(0.7, 1.5) * pose.scale;
    if size < 1.0 {
        return;
    }
    let alpha = (pose.alpha * style.opacity).clamp(0.0, 1.0);
    let fill = |c: Rgba| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]).gamma_multiply(alpha);
    let font = FontId::proportional(size);
    let text = "ข้อความ";
    let galley = p.layout_no_wrap(text.to_owned(), font.clone(), fill(style.color));
    // ระยะเลื่อนขยายให้เห็นชัดในการ์ดเล็ก ๆ
    let center = rect.center() + egui::vec2(pose.dx, pose.dy) * rect.height() * 3.0;
    let pos = center - galley.size() / 2.0;
    if style.background {
        let pad = size * style.box_padding / 100.0;
        p.rect_filled(egui::Rect::from_min_size(pos, galley.size()).expand(pad), 0, fill(style.background_color));
    }
    if style.shadow {
        let (sx, sy) = style.shadow_xy();
        let g = p.layout_no_wrap(text.to_owned(), font.clone(), fill(style.shadow_color));
        p.galley(pos + egui::vec2(sx, sy) * size / 100.0, g, Color32::TRANSPARENT);
    }
    if style.outline > 0.01 {
        let o = (size * style.outline / 100.0).max(0.7);
        let g = p.layout_no_wrap(text.to_owned(), font, fill(style.outline_color));
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7), (0.7, 0.7)] {
            p.galley(pos + egui::vec2(dx, dy) * o, g.clone(), Color32::TRANSPARENT);
        }
    }
    p.galley(pos, galley, Color32::TRANSPARENT);
}

// ---------------- เสียง ----------------

fn voice_icon(v: Voice) -> Color32 {
    match v {
        Voice::None => theme::SURFACE_HOVER,
        Voice::Chipmunk => Color32::from_rgb(0xe8, 0x9b, 0x3c),
        Voice::Deep => Color32::from_rgb(0x3c, 0x4f, 0x9b),
        Voice::Robot => Color32::from_rgb(0x7a, 0x8a, 0x99),
        Voice::Phone => Color32::from_rgb(0x4c, 0x9b, 0x6a),
        Voice::Echo => Color32::from_rgb(0x8a, 0x5c, 0xc4),
        Voice::Hall => Color32::from_rgb(0xb0, 0x5c, 0x7a),
    }
}

fn audio_tab(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    section(ui, &format!("{}  เปลี่ยนเสียง", icons::MICROPHONE));
    let target = target_line(app, ui, Want::Sound);
    let cur = target.and_then(|id| app.timeline.clip(id)).map(|c| c.fx.voice);
    if let Some(v) = grid(ui, Voice::ALL, cur, |v| v.label().to_owned(), voice_icon) {
        if let Some(c) = app.selected_where(&ctx, Want::Sound) {
            app.edit(|tl| tl.update_clip(c.id, |c| c.fx.voice = v));
        }
    }
    hint(ui, "ระดับเสียง เฟด เปลี่ยนคีย์ เสียงทุ้ม/แหลม ลดเสียงรบกวน ปรับได้ที่แผงขวาเมื่อเลือกคลิป");

    section(ui, &format!("{}  เพลง/เสียงในคลัง", icons::MUSIC_NOTES));
    let songs: Vec<(std::path::PathBuf, String, Option<f64>)> = app
        .library
        .items
        .iter()
        .filter(|it| it.kind == crate::core::media::MediaKind::Audio)
        .map(|it| (it.path.clone(), it.name.clone(), it.info().and_then(|i| i.duration)))
        .collect();
    if songs.is_empty() {
        hint(ui, "นำเข้าไฟล์เพลงที่แท็บ \"สื่อ\" ก่อน");
    }
    for (path, name, dur) in songs {
        ui.horizontal(|ui| {
            if ui.button(icons::PLUS).on_hover_text("ใส่ที่หัวเล่น").clicked() {
                let at = app.playhead;
                app.add_to_timeline(&ctx, &path, None, Some(at));
            }
            ui.label(super::ellipsize(&name, 30));
            if let Some(d) = dur {
                ui.label(RichText::new(crate::core::time::format_short(d)).color(theme::TEXT_DIM));
            }
        });
    }
}

// ---------------- เอฟเฟกต์ / ทรานซิชัน / ฟิลเตอร์ ----------------

fn effect_color(e: Effect) -> Color32 {
    match e {
        Effect::None => theme::SURFACE_HOVER,
        Effect::Vignette => Color32::from_rgb(0x2a, 0x22, 0x3a),
        Effect::Blur => Color32::from_rgb(0x6f, 0x86, 0xa8),
        Effect::Sharpen => Color32::from_rgb(0xd8, 0xd8, 0xe0),
        Effect::Grain => Color32::from_rgb(0x8a, 0x7f, 0x70),
        Effect::OldFilm => Color32::from_rgb(0x9c, 0x7c, 0x52),
        Effect::Cinema => Color32::from_rgb(0x18, 0x18, 0x18),
        Effect::Mirror => Color32::from_rgb(0x3f, 0x8f, 0xa8),
        Effect::Glow => Color32::from_rgb(0xf0, 0xd8, 0x90),
    }
}

fn effects_tab(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    section(ui, &format!("{}  เอฟเฟกต์ภาพ", icons::SPARKLE));
    let target = target_line(app, ui, Want::Visual);
    let cur = target.and_then(|id| app.timeline.clip(id)).map(|c| c.fx.effect);
    if let Some(e) = grid(ui, Effect::ALL, cur, |e| e.label().to_owned(), effect_color) {
        if let Some(c) = app.selected_where(&ctx, Want::Visual) {
            app.edit(|tl| tl.update_clip(c.id, |c| c.fx.effect = e));
        }
    }
    let amount = target.and_then(|id| app.timeline.clip(id)).map(|c| c.fx.effect_amount);
    if let (Some(id), Some(e), Some(a)) = (target, cur, amount) {
        if !matches!(e, Effect::None | Effect::Mirror) {
            ui.horizontal(|ui| {
                ui.label("ความแรง");
                let mut v = a * 100.0;
                let resp = ui.add(Fader::new(&mut v, 0.0..=200.0).step(1.0).suffix("%").default_value(100.0));
                app.live_edit(&resp, |tl| tl.update_clip(id, |c| c.fx.effect_amount = v / 100.0));
            });
            hint(ui, "100% = ค่าปกติ");
        }
    }
}

fn transitions_tab(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    section(ui, &format!("{}  ทรานซิชัน", icons::INTERSECT));
    hint(ui, "เลือกคลิปในแทร็กหลัก ทรานซิชันจะอยู่ระหว่างคลิปนั้นกับคลิปถัดไป (หรือคลิกรูปเพชรตรงรอยต่อในไทม์ไลน์)");
    let target = target_line(app, ui, Want::Main);
    let idx = target.and_then(|id| app.timeline.main.iter().position(|c| c.id == id));
    let is_last = idx.is_some_and(|i| i + 1 >= app.timeline.main.len());
    if is_last {
        ui.label(RichText::new("คลิปนี้เป็นคลิปสุดท้าย ไม่มีคลิปถัดไปให้ต่อ").color(theme::WARN).size(13.0));
    }
    let cur = target.and_then(|id| app.timeline.clip(id)).and_then(|c| c.fx.transition);
    let picked = grid(ui, TransitionKind::ALL, cur.map(|t| t.kind), |k| k.label().to_owned(), |_| theme::ACCENT_DARK);
    if let Some(kind) = picked {
        match (app.selected_where(&ctx, Want::Main), is_last) {
            (Some(_), true) => app.notify(&ctx, "เลือกคลิปที่ไม่ใช่คลิปสุดท้าย"),
            (Some(c), false) => {
                let dur = cur.map_or(TRANSITION_DEFAULT, |t| t.dur);
                app.edit(|tl| tl.update_clip(c.id, |c| c.fx.transition = Some(Transition { kind, dur })));
            }
            (None, _) => {}
        }
    }
    if let (Some(id), Some(t)) = (target, cur) {
        ui.horizontal(|ui| {
            ui.label("ความยาว");
            let mut d = t.dur;
            let resp = ui.add(Fader::new(&mut d, 0.1..=3.0).step(0.05).suffix(" วิ").default_value(TRANSITION_DEFAULT));
            app.live_edit(&resp, |tl| tl.update_clip(id, |c| c.fx.transition = Some(Transition { dur: d, ..t })));
        });
        hint(ui, "ของจริงยาวได้ไม่เกินครึ่งหนึ่งของคลิปที่สั้นกว่า");
        ui.horizontal(|ui| {
            if ui.button(format!("{} ใช้กับทุกรอยต่อ", icons::INTERSECT)).on_hover_text("ใส่ทรานซิชันแบบนี้และความยาวนี้ทุกรอยต่อในแทร็กหลัก").clicked()
            {
                app.edit(|tl| {
                    let n = tl.main.len();
                    for c in tl.main.iter_mut().take(n.saturating_sub(1)) {
                        c.fx.transition = Some(t);
                    }
                });
                app.notify(&ctx, "ใส่ทรานซิชันทุกรอยต่อแล้ว");
            }
            if ui.button(format!("{} เอาออก", icons::X_CIRCLE)).clicked() {
                app.edit(|tl| tl.update_clip(id, |c| c.fx.transition = None));
            }
        });
    }
    if app.timeline.main.iter().any(|c| c.fx.transition.is_some())
        && ui.small_button(format!("{} เอาทรานซิชันออกทั้งหมด", icons::TRASH)).clicked()
    {
        app.edit(|tl| tl.main.iter_mut().for_each(|c| c.fx.transition = None));
    }
}

fn filter_color(f: FilterPreset) -> Color32 {
    match f {
        FilterPreset::None => theme::SURFACE_HOVER,
        FilterPreset::Vivid => Color32::from_rgb(0xff, 0x5a, 0x6e),
        FilterPreset::Warm => Color32::from_rgb(0xe8, 0x9b, 0x4a),
        FilterPreset::Cool => Color32::from_rgb(0x5a, 0x9b, 0xe0),
        FilterPreset::Film => Color32::from_rgb(0x3d, 0x7a, 0x78),
        FilterPreset::Vintage => Color32::from_rgb(0xb8, 0x9a, 0x6a),
        FilterPreset::Fade => Color32::from_rgb(0xb0, 0xae, 0xb8),
        FilterPreset::Sepia => Color32::from_rgb(0x8f, 0x6a, 0x45),
        FilterPreset::Mono => Color32::from_rgb(0x88, 0x88, 0x88),
        FilterPreset::Noir => Color32::from_rgb(0x30, 0x30, 0x30),
    }
}

fn filters_tab(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    section(ui, &format!("{}  ฟิลเตอร์", icons::CIRCLE_HALF));
    let target = target_line(app, ui, Want::Visual);
    let fx = target.and_then(|id| app.timeline.clip(id)).map(|c| c.fx.clone());
    if let Some(f) = grid(ui, FilterPreset::ALL, fx.as_ref().map(|f| f.filter), |f| f.label().to_owned(), filter_color) {
        if let Some(c) = app.selected_where(&ctx, Want::Visual) {
            app.edit(|tl| tl.update_clip(c.id, |c| c.fx.filter = f));
        }
    }
    if let (Some(id), Some(fx)) = (target, fx.filter(|f| f.filter != FilterPreset::None)) {
        ui.horizontal(|ui| {
            ui.label("ความเข้ม");
            let mut a = fx.filter_amount * 100.0;
            let resp = ui.add(Fader::new(&mut a, 0.0..=100.0).step(1.0).suffix("%").default_value(100.0));
            app.live_edit(&resp, |tl| tl.update_clip(id, |c| c.fx.filter_amount = a / 100.0));
        });
    }
}

// ---------------- ปรับแต่งสี ----------------

/// ตัวเลื่อนปรับค่า -1..1 (แสดงเป็น -100..100)
#[allow(clippy::too_many_arguments)]
fn adjust_slider(
    app: &mut TadTorApp,
    ui: &mut egui::Ui,
    id: u64,
    icon: &str,
    name: &str,
    value: f32,
    range: (f32, f32),
    set: fn(&mut ClipFx, f32),
) {
    ui.horizontal(|ui| {
        ui.add_sized([104.0, 26.0], egui::Label::new(RichText::new(format!("{icon} {name}")).color(theme::TEXT_DIM)));
        let mut v = value * 100.0;
        let resp = ui.add(Fader::new(&mut v, range.0 * 100.0..=range.1 * 100.0).step(1.0).default_value(0.0));
        app.live_edit(&resp, |tl| tl.update_clip(id, |c| set(&mut c.fx, v / 100.0)));
    });
}

fn adjust_tab(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    section(ui, &format!("{}  ปรับแต่งภาพ", icons::SLIDERS_HORIZONTAL));
    let Some(id) = target_line(app, ui, Want::Visual) else { return };
    let Some(fx) = app.timeline.clip(id).map(|c| c.fx.clone()) else { return };
    hint(ui, "กด ↺ หรือดับเบิลคลิกตัวเลื่อนเพื่อคืนค่า 0 • Shift + ลาก = ปรับละเอียด");
    adjust_slider(app, ui, id, icons::SUN, "ความสว่าง", fx.brightness, (-1.0, 1.0), |f, v| f.brightness = v);
    adjust_slider(app, ui, id, icons::CIRCLE_HALF_TILT, "คอนทราสต์", fx.contrast, (-1.0, 1.0), |f, v| f.contrast = v);
    adjust_slider(app, ui, id, icons::DROP, "ความอิ่มสี", fx.saturation, (-1.0, 1.0), |f, v| f.saturation = v);
    adjust_slider(app, ui, id, icons::THERMOMETER, "อุณหภูมิสี", fx.temperature, (-1.0, 1.0), |f, v| f.temperature = v);
    adjust_slider(app, ui, id, icons::DIAMOND, "ความคม", fx.sharpen, (0.0, 1.0), |f, v| f.sharpen = v);
    adjust_slider(app, ui, id, icons::CIRCLE_HALF, "ขอบมืด", fx.vignette, (0.0, 1.0), |f, v| f.vignette = v);

    section(ui, &format!("{}  สีและแสงละเอียด", icons::PALETTE));
    adjust_slider(app, ui, id, icons::PALETTE, "เฉดสี", fx.hue, (-1.0, 1.0), |f, v| f.hue = v);
    adjust_slider(app, ui, id, icons::DROP, "เขียว-ม่วง", fx.tint, (-1.0, 1.0), |f, v| f.tint = v);
    adjust_slider(app, ui, id, icons::SUN, "ส่วนสว่าง", fx.highlights, (-1.0, 1.0), |f, v| f.highlights = v);
    adjust_slider(app, ui, id, icons::CIRCLE_HALF_TILT, "ส่วนมืด", fx.shadows, (-1.0, 1.0), |f, v| f.shadows = v);
    adjust_slider(app, ui, id, icons::CIRCLE_HALF, "จางฟิล์ม", fx.fade, (0.0, 1.0), |f, v| f.fade = v);
    adjust_slider(app, ui, id, icons::SPARKLE, "เม็ดฟิล์ม", fx.grain, (0.0, 1.0), |f, v| f.grain = v);
    hint(ui, "เฉดสี 100 = หมุนสี 180°");

    section(ui, &format!("{}  LUT (.cube)", icons::PALETTE));
    match &fx.lut {
        Some(p) => {
            ui.horizontal(|ui| {
                ui.label(super::ellipsize(&crate::core::media::file_name(p), 28)).on_hover_text(p.display().to_string());
                if ui.small_button(format!("{} เอาออก", icons::X_CIRCLE)).clicked() {
                    app.edit(|tl| tl.update_clip(id, |c| c.fx.lut = None));
                }
            });
            if !p.is_file() {
                ui.label(RichText::new("หาไฟล์ LUT ไม่เจอ").color(theme::ERROR).size(13.0));
            }
        }
        None => hint(ui, "ใช้ไฟล์ LUT สีสำเร็จรูป (.cube) ที่หามาเอง"),
    }
    if ui.button(format!("{} เลือกไฟล์ LUT…", icons::FOLDER_OPEN)).clicked() {
        if let Some(p) = rfd::FileDialog::new().set_title("เลือกไฟล์ LUT").add_filter("LUT", &["cube"]).pick_file() {
            app.edit(|tl| tl.update_clip(id, |c| c.fx.lut = Some(p)));
        }
    }

    ui.add_space(10.0);
    if ui.button(format!("{} คืนค่าการปรับแต่งทั้งหมด", icons::ARROW_COUNTER_CLOCKWISE)).clicked()
    {
        app.edit(|tl| tl.update_clip(id, |c| c.fx.reset_adjust()));
        app.notify(&ctx, "คืนค่าแล้ว");
    }
}
