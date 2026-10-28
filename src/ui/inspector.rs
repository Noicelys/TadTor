use std::path::Path;

use eframe::egui::{self, RichText};

use super::fader::Fader;
use crate::app::{LeftTab, TadTorApp};
use crate::core::fx::{ClipFx, Easing, Effect, FilterPreset, Fit, Motion, Rgba, TextAlign, TextLoop, TextMotion, TextStyle, Voice};
use crate::core::media::{MediaKind, MediaState};
use crate::core::project::AspectRatio;
use crate::core::time::format_timecode;
use crate::core::timeline::{Clip, TrackKind, OVERLAY_ZOOM};
use crate::ffmpeg::FfmpegStatus;
use crate::{fonts, icons, theme};

/// การ์ดรายละเอียด (วางในช่องไหนก็ได้ ดู `layout`)
pub fn content(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let is_text = app.selected_clip.and_then(|id| app.timeline.clip(id)).is_some_and(|c| c.text.is_some());
    let title = if is_text {
        "แก้ข้อความ"
    } else if app.selected_clip.is_some() {
        "รายละเอียดคลิป"
    } else if app.selected_media.is_some() {
        "รายละเอียดไฟล์"
    } else {
        "รายละเอียดโปรเจกต์"
    };
    super::panel_header(ui, title, |ui| super::layout::grip(app, ui, super::layout::Card::Inspector));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
            if let Some(id) = app.selected_clip {
                clip_details(app, ui, id);
            } else if let Some(path) = app.selected_media.clone() {
                media_details(app, ui, &path);
            } else {
                project_settings(app, ui);
            }
            ui.add_space(18.0);
            system_status(app, ui);
        });
    });
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_sized([108.0, 26.0], egui::Label::new(RichText::new(label).color(theme::TEXT_DIM)));
        add(ui);
    })
    .response
}

fn project_settings(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let before = app.project.clone();
    project_fields(app, ui);
    if app.project != before {
        app.dirty = true;
        app.stop_playback();
    }
    ui.add_space(10.0);
    let d = app.timeline.duration();
    row(ui, "ความยาวงาน", |ui| {
        ui.label(format_timecode(d, app.project.fps));
    });
    row(ui, "คลิป", |ui| {
        let tl = &app.timeline;
        ui.label(format!("ภาพ {} • ภาพซ้อน {} • เสียง {}", tl.main.len(), tl.overlay.len(), tl.audio.len()));
    });
}

fn project_fields(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let p = &mut app.project;
    row(ui, "ชื่อ", |ui| {
        ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(f32::INFINITY));
    });
    row(ui, "อัตราส่วน", |ui| {
        egui::ComboBox::from_id_salt("insp_aspect").selected_text(p.aspect.label()).show_ui(ui, |ui| {
            for a in AspectRatio::ALL {
                ui.selectable_value(&mut p.aspect, a, a.label());
            }
        });
    });
    row(ui, "คุณภาพ", |ui| {
        let label = |q: u32| match q {
            720 => "720p",
            1080 => "1080p",
            1440 => "2K",
            2160 => "4K",
            _ => "อื่น ๆ",
        };
        egui::ComboBox::from_id_salt("insp_quality").selected_text(label(p.quality)).show_ui(ui, |ui| {
            for q in [720, 1080, 1440, 2160] {
                ui.selectable_value(&mut p.quality, q, label(q));
            }
        });
    });
    row(ui, "เฟรมเรต", |ui| {
        egui::ComboBox::from_id_salt("insp_fps").selected_text(format!("{} fps", p.fps)).show_ui(ui, |ui| {
            for f in [24, 25, 30, 50, 60] {
                ui.selectable_value(&mut p.fps, f, format!("{f} fps"));
            }
        });
    });
    let (w, h) = p.resolution();
    row(ui, "ขนาดภาพ", |ui| {
        ui.label(format!("{w} × {h}"));
    });
}

fn media_details(app: &mut TadTorApp, ui: &mut egui::Ui, path: &Path) {
    let Some(item) = app.library.get(path) else {
        app.selected_media = None;
        return;
    };
    row(ui, "ชื่อไฟล์", |ui| {
        ui.add(egui::Label::new(&item.name).wrap());
    });
    row(ui, "ประเภท", |ui| {
        ui.label(format!("{} {}", item.kind.icon(), item.kind.label()));
    });
    match &item.state {
        MediaState::Probing => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("กำลังอ่านไฟล์…");
            });
        }
        MediaState::Failed(e) => {
            ui.horizontal(|ui| {
                super::status_dot(ui, theme::ERROR);
                ui.add(egui::Label::new(RichText::new(format!("ใช้ไฟล์นี้ไม่ได้: {e}")).color(theme::ERROR)).wrap());
            });
        }
        MediaState::Ready(info) => {
            if let Some(d) = info.duration {
                row(ui, "ความยาว", |ui| {
                    ui.label(format_timecode(d, app.project.fps));
                });
            }
            if info.has_video {
                row(ui, "ขนาดภาพ", |ui| {
                    ui.label(format!("{} × {}", info.width, info.height));
                });
            }
            if let Some(fps) = info.fps.filter(|_| item.kind == MediaKind::Video) {
                row(ui, "เฟรมเรต", |ui| {
                    ui.label(format!("{fps:.2} fps"));
                });
            }
            if item.kind != MediaKind::Image {
                row(ui, "เสียง", |ui| {
                    ui.label(if info.has_audio { "มี" } else { "ไม่มี" });
                });
            }
        }
    }
    row(ui, "ที่อยู่", |ui| {
        ui.add(egui::Label::new(RichText::new(item.path.display().to_string()).size(12.5)).wrap());
    });
    ui.add_space(8.0);
    let ready = matches!(item.state, MediaState::Ready(_));
    let visual = item.kind.is_visual();
    ui.horizontal(|ui| {
        let ctx = ui.ctx().clone();
        if ui.add_enabled(ready, egui::Button::new(format!("{} ใส่ลงไทม์ไลน์", icons::PLUS))).clicked() {
            app.add_to_timeline(&ctx, path, None, None);
        }
        if visual
            && ui
                .add_enabled(ready, egui::Button::new(format!("{} ซ้อนที่หัวเล่น", icons::IMAGE)))
                .on_hover_text("วางบนแทร็กภาพซ้อน ทับภาพในแทร็กหลัก (ภาพซ้อนเล็ก โลโก้ สติกเกอร์)")
                .clicked()
        {
            let at = app.playhead;
            app.add_overlay(&ctx, path, at);
        }
        if ui.button("เอาออกจากคลัง").clicked() {
            app.remove_media(path);
        }
    });
}

fn clip_details(app: &mut TadTorApp, ui: &mut egui::Ui, id: u64) {
    let Some(c) = app.timeline.clip(id).cloned() else {
        app.selected_clip = None;
        return;
    };
    let track = app.timeline.find(id).map(|f| f.0).unwrap_or(TrackKind::Main);
    let fps = app.project.fps;
    let ctx = ui.ctx().clone();

    if let Some(style) = &c.text {
        text_editor(app, ui, id, style);
        ui.add_space(8.0);
    } else {
        row(ui, "ชื่อ", |ui| {
            ui.add(egui::Label::new(&c.name).wrap());
        });
        row(ui, "ประเภท", |ui| {
            ui.label(format!("{} {}", c.kind.icon(), c.kind.label()));
        });
    }
    row(ui, "เริ่มที่", |ui| {
        ui.label(format_timecode(c.start, fps));
    });
    if c.src_len.is_none() {
        // รูปภาพ/ข้อความ ตั้งความยาวได้อิสระ
        row(ui, "ความยาว", |ui| {
            let mut d = c.dur;
            let resp = ui.add(egui::DragValue::new(&mut d).range(0.1..=3600.0).speed(0.05).suffix(" วิ").max_decimals(2));
            app.live_edit(&resp, |tl| tl.set_duration(id, d));
        });
    } else {
        row(ui, "ความยาว", |ui| {
            ui.label(format_timecode(c.dur, fps));
        });
        row(ui, "ช่วงในไฟล์", |ui| {
            ui.label(format!("{} – {}", format_timecode(c.src_in, fps), format_timecode(c.src_in + c.dur, fps)));
        });
    }
    if c.kind == MediaKind::Video && !c.has_audio {
        ui.label(RichText::new("วิดีโอนี้ไม่มีเสียง").color(theme::TEXT_DIM).size(12.5));
    }

    // ---- เสียง ----
    if c.has_audio {
        section(ui, &format!("{} เสียง", icons::SPEAKER_HIGH));
        row(ui, "ระดับเสียง", |ui| {
            let mut v = c.fx.volume * 100.0;
            let resp = ui.add(Fader::new(&mut v, 0.0..=200.0).step(1.0).suffix("%").default_value(100.0));
            app.live_edit(&resp, |tl| tl.update_clip(id, |c| c.fx.volume = v / 100.0));
        });
        // ภาพซ้อนตั้งเฟดในหัวข้อ "ภาพซ้อน" (เฟดทั้งภาพและเสียง)
        if track != TrackKind::Overlay {
            fade_rows(app, ui, id, &c);
        }
        row(ui, "เปลี่ยนเสียง", |ui| {
            let mut v = c.fx.voice;
            egui::ComboBox::from_id_salt("insp_voice").selected_text(v.label()).show_ui(ui, |ui| {
                for &o in Voice::ALL {
                    ui.selectable_value(&mut v, o, o.label());
                }
            });
            if v != c.fx.voice {
                app.edit(|tl| tl.update_clip(id, |c| c.fx.voice = v));
            }
        });
        row(ui, "เปลี่ยนคีย์", |ui| {
            fx_slider(app, ui, id, c.fx.pitch, -12.0..=12.0, 0.5, "", 0.0, |f, v| f.pitch = v)
        })
        .on_hover_text("ครึ่งเสียง (+12 = สูงขึ้นหนึ่งช่วงคู่แปด) ความเร็วเท่าเดิม");
        row(ui, "เสียงทุ้ม", |ui| fx_slider(app, ui, id, c.fx.bass, -12.0..=12.0, 0.5, " dB", 0.0, |f, v| f.bass = v));
        row(ui, "เสียงแหลม", |ui| {
            fx_slider(app, ui, id, c.fx.treble, -12.0..=12.0, 0.5, " dB", 0.0, |f, v| f.treble = v)
        });
        row(ui, "ลดเสียงรบกวน", |ui| {
            fx_slider(app, ui, id, c.fx.denoise * 100.0, 0.0..=100.0, 1.0, "%", 0.0, |f, v| f.denoise = v / 100.0)
        })
        .on_hover_text("ลดเสียงซ่า เสียงพัดลม เสียงฮัม (แรงมากเสียงพูดอาจเพี้ยน)");
        ui.horizontal(|ui| {
            dim(ui, "ดับเบิลคลิกตัวเลื่อนเพื่อคืนค่า");
            if c.fx.has_sound_fx() && ui.small_button(format!("{} คืนค่าเสียง", icons::ARROW_COUNTER_CLOCKWISE)).clicked()
            {
                app.edit(|tl| tl.update_clip(id, |c| c.fx.reset_sound_fx()));
            }
        });
    }

    // ---- จัดวางภาพ ----
    if track == TrackKind::Main {
        layout_editor(app, ui, id, &c.fx);
    }
    if track == TrackKind::Overlay {
        overlay_editor(app, ui, id, &c);
    }

    // ---- ภาพ (สรุป + ปุ่มไปแท็บที่แก้) ----
    if track.is_visual() {
        section(ui, &format!("{} ภาพ", icons::PALETTE));
        let mut go = None;
        let mut line = |ui: &mut egui::Ui, label: &str, value: String, tab: LeftTab| {
            row(ui, label, |ui| {
                ui.label(value);
                if ui.small_button("แก้").clicked() {
                    go = Some(tab);
                }
            });
        };
        let filter = if c.fx.filter == FilterPreset::None {
            "ไม่มี".to_owned()
        } else {
            format!("{} {:.0}%", c.fx.filter.label(), c.fx.filter_amount * 100.0)
        };
        line(ui, "ฟิลเตอร์", filter, LeftTab::Filters);
        let adjusted = if c.fx.has_adjust() || c.fx.lut.is_some() { "มี" } else { "ไม่มี" };
        line(ui, "ปรับแต่ง", adjusted.to_owned(), LeftTab::Adjust);
        let effect = match c.fx.effect {
            Effect::None | Effect::Mirror => c.fx.effect.label().to_owned(),
            e => format!("{} {:.0}%", e.label(), c.fx.effect_amount * 100.0),
        };
        line(ui, "เอฟเฟกต์", effect, LeftTab::Effects);
        let is_last = app.timeline.main.last().map(|l| l.id) == Some(id);
        if track == TrackKind::Main && !is_last {
            let tr = match c.fx.transition {
                Some(t) => format!("{} {:.1} วิ", t.kind.label(), t.dur),
                None => "ไม่มี".to_owned(),
            };
            line(ui, "ทรานซิชัน", tr, LeftTab::Transitions);
        }
        if let Some(tab) = go {
            app.left_tab = tab;
        }
    }

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui.button(format!("{} แยกที่หัวเล่น", icons::SCISSORS)).clicked() {
            app.split_at_playhead(&ctx);
        }
        if ui.button(format!("{} ลบ", icons::TRASH)).clicked() {
            app.delete_selected();
        }
    });
    let toggle_label = match track {
        TrackKind::Main => Some("ย้ายไปซ้อนบนภาพ"),
        TrackKind::Overlay => Some("ย้ายลงแทร็กหลัก"),
        _ => None,
    };
    if let Some(label) = toggle_label {
        if ui.button(format!("{} {label}", icons::IMAGE)).on_hover_text("ลากคลิปขึ้น/ลงระหว่างแทร็กในไทม์ไลน์ก็ได้").clicked()
        {
            app.toggle_clip_overlay(id);
        }
    }
    ui.add_space(4.0);
    ui.label(RichText::new("ลากขอบซ้าย/ขวาของคลิปเพื่อตัดหัว/ท้าย • ลากกลางคลิปเพื่อย้าย").color(theme::TEXT_DIM).size(12.5));
}

/// เฟดเข้า/ออก (เสียง และภาพของภาพซ้อน)
fn fade_rows(app: &mut TadTorApp, ui: &mut egui::Ui, id: u64, c: &Clip) {
    let max_fade = (c.dur / 2.0).clamp(0.1, 10.0);
    row(ui, "เฟดเข้า", |ui| {
        let mut f = c.fx.fade_in;
        let resp = ui.add(Fader::new(&mut f, 0.0..=max_fade).step(0.1).suffix(" วิ").default_value(0.0));
        app.live_edit(&resp, |tl| tl.update_clip(id, |c| c.fx.fade_in = f));
    });
    row(ui, "เฟดออก", |ui| {
        let mut f = c.fx.fade_out;
        let resp = ui.add(Fader::new(&mut f, 0.0..=max_fade).step(0.1).suffix(" วิ").default_value(0.0));
        app.live_edit(&resp, |tl| tl.update_clip(id, |c| c.fx.fade_out = f));
    });
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).strong());
    ui.separator();
}

fn dim(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::TEXT_DIM).size(12.5));
}

/// แถบเลื่อนหนึ่งค่าของคลิป (ลาก = แก้สด ๆ เก็บ Undo ครั้งเดียว, ดับเบิลคลิก/ปุ่ม ↺ = คืนค่า `reset`)
#[allow(clippy::too_many_arguments)]
fn fx_slider(
    app: &mut TadTorApp,
    ui: &mut egui::Ui,
    id: u64,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    suffix: &str,
    reset: f32,
    set: fn(&mut ClipFx, f32),
) {
    let mut v = value;
    let resp = ui.add(Fader::new(&mut v, range).step(step).suffix(suffix).default_value(reset));
    app.live_edit(&resp, |tl| tl.update_clip(id, |c| set(&mut c.fx, v)));
    if resp.double_clicked() {
        app.edit(|tl| tl.update_clip(id, |c| set(&mut c.fx, reset)));
    }
}

/// แบบวาง ซูม เลื่อน หมุน กลับด้าน สีพื้นหลัง ของคลิปในแทร็กหลัก
fn layout_editor(app: &mut TadTorApp, ui: &mut egui::Ui, id: u64, fx: &ClipFx) {
    section(ui, &format!("{} จัดวางภาพ", icons::IMAGE));
    fn set(app: &mut TadTorApp, id: u64, f: impl FnOnce(&mut ClipFx)) {
        app.edit(|tl| tl.update_clip(id, |c| f(&mut c.fx)));
    }
    row(ui, "แบบวาง", |ui| {
        for &f in Fit::ALL {
            if ui.add(egui::Button::selectable(fx.fit == f, f.label())).clicked() {
                set(app, id, |x| x.fit = f);
            }
        }
    });
    row(ui, "ซูม", |ui| fx_slider(app, ui, id, fx.zoom * 100.0, 10.0..=400.0, 1.0, "%", 100.0, |f, v| f.zoom = v / 100.0));
    row(ui, "เลื่อนซ้าย-ขวา", |ui| {
        fx_slider(app, ui, id, fx.pan_x * 100.0, -100.0..=100.0, 1.0, "%", 0.0, |f, v| f.pan_x = v / 100.0)
    });
    row(ui, "เลื่อนบน-ล่าง", |ui| {
        fx_slider(app, ui, id, fx.pan_y * 100.0, -100.0..=100.0, 1.0, "%", 0.0, |f, v| f.pan_y = v / 100.0)
    });
    row(ui, "หมุน", |ui| {
        for q in 0..4u8 {
            if ui.add(egui::Button::selectable(fx.rotate % 4 == q, format!("{}°", q as u32 * 90))).clicked() {
                set(app, id, |x| x.rotate = q);
            }
        }
    });
    row(ui, "กลับด้าน", |ui| {
        if ui.add(egui::Button::selectable(fx.flip_h, "ซ้าย-ขวา")).clicked() {
            set(app, id, |x| x.flip_h = !x.flip_h);
        }
        if ui.add(egui::Button::selectable(fx.flip_v, "บน-ล่าง")).clicked() {
            set(app, id, |x| x.flip_v = !x.flip_v);
        }
    });
    row(ui, "สีพื้นหลัง", |ui| {
        let mut c = [fx.background[0], fx.background[1], fx.background[2]];
        let resp = ui.color_edit_button_srgb(&mut c);
        app.live_edit(&resp, |tl| tl.update_clip(id, |x| x.fx.background = [c[0], c[1], c[2], 255]));
        dim(ui, "ขอบว่างรอบภาพ");
    });
    if fx.fit == Fit::Contain && fx.zoom <= 1.0 && (fx.pan_x != 0.0 || fx.pan_y != 0.0) {
        dim(ui, "เลื่อนได้เมื่อภาพเล็กกว่าหรือใหญ่กว่าจอ");
    }
    if fx.has_layout() && ui.small_button(format!("{} คืนค่าการจัดวาง", icons::ARROW_COUNTER_CLOCKWISE)).clicked()
    {
        set(app, id, ClipFx::reset_layout);
    }
}

/// ขนาด ตำแหน่ง ความทึบ เฟด หมุน กลับด้าน ของคลิปในแทร็กภาพซ้อน
fn overlay_editor(app: &mut TadTorApp, ui: &mut egui::Ui, id: u64, c: &Clip) {
    let fx = &c.fx;
    section(ui, &format!("{} ภาพซ้อน", icons::IMAGE));
    fn set(app: &mut TadTorApp, id: u64, f: impl FnOnce(&mut ClipFx)) {
        app.edit(|tl| tl.update_clip(id, |c| f(&mut c.fx)));
    }
    row(ui, "ขนาด", |ui| {
        fx_slider(app, ui, id, fx.zoom * 100.0, 5.0..=200.0, 1.0, "%", OVERLAY_ZOOM * 100.0, |f, v| f.zoom = v / 100.0)
    })
    .on_hover_text("100% = พอดีจอ");
    row(ui, "วางที่", |ui| {
        const SPOTS: [(&str, f32, f32); 5] =
            [("ซ้ายบน", -0.9, -0.9), ("ขวาบน", 0.9, -0.9), ("กลาง", 0.0, 0.0), ("ซ้ายล่าง", -0.9, 0.9), ("ขวาล่าง", 0.9, 0.9)];
        for (label, x, y) in SPOTS {
            if ui.small_button(label).clicked() {
                set(app, id, |f| {
                    f.pan_x = x;
                    f.pan_y = y;
                });
            }
        }
    });
    row(ui, "ซ้าย-ขวา", |ui| {
        fx_slider(app, ui, id, fx.pan_x * 100.0, -100.0..=100.0, 1.0, "%", 0.0, |f, v| f.pan_x = v / 100.0)
    });
    row(ui, "บน-ล่าง", |ui| {
        fx_slider(app, ui, id, fx.pan_y * 100.0, -100.0..=100.0, 1.0, "%", 0.0, |f, v| f.pan_y = v / 100.0)
    });
    dim(ui, "ลากภาพในเครื่องเล่นเพื่อย้ายได้เลย");
    row(ui, "ความทึบ", |ui| {
        fx_slider(app, ui, id, fx.opacity * 100.0, 0.0..=100.0, 1.0, "%", 100.0, |f, v| f.opacity = v / 100.0)
    });
    fade_rows(app, ui, id, c);
    row(ui, "หมุน", |ui| {
        for q in 0..4u8 {
            if ui.add(egui::Button::selectable(fx.rotate % 4 == q, format!("{}°", q as u32 * 90))).clicked() {
                set(app, id, |x| x.rotate = q);
            }
        }
    });
    row(ui, "กลับด้าน", |ui| {
        if ui.add(egui::Button::selectable(fx.flip_h, "ซ้าย-ขวา")).clicked() {
            set(app, id, |x| x.flip_h = !x.flip_h);
        }
        if ui.add(egui::Button::selectable(fx.flip_v, "บน-ล่าง")).clicked() {
            set(app, id, |x| x.flip_v = !x.flip_v);
        }
    });
    if ui.small_button(format!("{} คืนค่าภาพซ้อน", icons::ARROW_COUNTER_CLOCKWISE)).clicked() {
        set(app, id, |f| {
            f.reset_layout();
            f.zoom = OVERLAY_ZOOM;
            f.fade_in = 0.0;
            f.fade_out = 0.0;
        });
    }
}

fn rgba(c: Rgba) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// แก้ค่าหนึ่งของข้อความ (ลาก/พิมพ์ = แก้สด ๆ เก็บ Undo ครั้งเดียว)
fn set_text(app: &mut TadTorApp, id: u64, resp: &egui::Response, f: impl FnOnce(&mut TextStyle)) {
    app.live_edit(resp, |tl| {
        tl.update_clip(id, |c| {
            if let Some(t) = c.text.as_mut() {
                f(t);
            }
        })
    });
}

/// แถบเลื่อนหนึ่งค่าของข้อความ (`reset` = ค่าเริ่มต้นในหน่วยที่แสดง)
#[allow(clippy::too_many_arguments)]
fn text_slider(
    app: &mut TadTorApp,
    ui: &mut egui::Ui,
    id: u64,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    suffix: &str,
    reset: f32,
    set: fn(&mut TextStyle, f32),
) {
    let mut v = value;
    let resp = ui.add(Fader::new(&mut v, range).step(step).suffix(suffix).default_value(reset));
    set_text(app, id, &resp, |t| set(t, v));
}

/// ปุ่มเปิด/ปิดหนึ่งค่าของข้อความ (กดแล้วเก็บ Undo ทันที)
fn text_toggle(app: &mut TadTorApp, id: u64, resp: &egui::Response, on: bool, set: fn(&mut TextStyle, bool)) {
    if resp.clicked() || resp.changed() {
        app.edit(|tl| {
            tl.update_clip(id, |c| {
                if let Some(t) = c.text.as_mut() {
                    set(t, on);
                }
            })
        });
    }
}

fn text_color(app: &mut TadTorApp, ui: &mut egui::Ui, id: u64, value: Rgba, set: fn(&mut TextStyle, Rgba)) {
    let mut c = rgba(value);
    let resp = ui.color_edit_button_srgba(&mut c);
    set_text(app, id, &resp, |t| set(t, c.to_srgba_unmultiplied()));
}

/// แก้ข้อความและสไตล์
fn text_editor(app: &mut TadTorApp, ui: &mut egui::Ui, id: u64, style: &TextStyle) {
    ui.label(RichText::new(format!("{} ข้อความ", icons::TEXT_T)).strong());
    let mut content = style.content.clone();
    let resp = ui.add(
        egui::TextEdit::multiline(&mut content).desired_rows(3).desired_width(f32::INFINITY).hint_text("พิมพ์ข้อความ (Enter = ขึ้นบรรทัดใหม่)"),
    );
    set_text(app, id, &resp, |t| t.content = content);

    section(ui, &format!("{} ฟอนต์", icons::TEXT_AA));
    font_picker(app, ui, id, style);
    let family = if style.font.is_empty() || !fonts::ready() { None } else { fonts::find(&style.font) };
    row(ui, "แบบ", |ui| {
        let mut b = style.bold;
        let resp = ui.toggle_value(&mut b, RichText::new("B").strong()).on_hover_text("ตัวหนา");
        text_toggle(app, id, &resp, b, |t, v| t.bold = v);
        let italic_ok = family.is_some_and(|f| f.has_italic());
        let mut i = style.italic && italic_ok;
        let resp = ui
            .add_enabled(italic_ok, egui::Button::selectable(i, RichText::new("I").italics()))
            .on_hover_text("ตัวเอียง")
            .on_disabled_hover_text("ฟอนต์นี้ไม่มีแบบตัวเอียง");
        if resp.clicked() {
            i = !i;
        }
        text_toggle(app, id, &resp, i, |t, v| t.italic = v);
        if family.is_some_and(|f| style.bold && !f.has_bold()) {
            ui.label(RichText::new("ฟอนต์นี้ไม่มีตัวหนา").color(theme::TEXT_DIM).size(12.5));
        }
    });
    font_sample(ui, style);
    row(ui, "ขนาด", |ui| text_slider(app, ui, id, style.size, 1.0..=40.0, 0.1, "", 7.0, |t, v| t.size = v));
    row(ui, "จัดบรรทัด", |ui| {
        for &a in TextAlign::ALL {
            if ui.add(egui::Button::selectable(style.align == a, a.label())).clicked() {
                app.edit(|tl| tl.update_clip(id, |c| c.text.as_mut().unwrap().align = a));
            }
        }
    });
    row(ui, "ระยะบรรทัด", |ui| {
        text_slider(app, ui, id, style.line_spacing, 0.0..=150.0, 1.0, "%", 15.0, |t, v| t.line_spacing = v)
    });

    section(ui, &format!("{} สีและขอบ", icons::PALETTE));
    row(ui, "สีตัวอักษร", |ui| text_color(app, ui, id, style.color, |t, c| t.color = c));
    row(ui, "ความทึบ", |ui| {
        text_slider(app, ui, id, style.opacity * 100.0, 0.0..=100.0, 1.0, "%", 100.0, |t, v| t.opacity = v / 100.0)
    });
    row(ui, "ขอบ", |ui| {
        text_color(app, ui, id, style.outline_color, |t, c| t.outline_color = c);
        text_slider(app, ui, id, style.outline, 0.0..=30.0, 0.5, "%", 8.0, |t, v| t.outline = v);
    });
    row(ui, "เงา", |ui| {
        let mut on = style.shadow;
        let resp = ui.checkbox(&mut on, "");
        text_toggle(app, id, &resp, on, |t, v| t.shadow = v);
        text_color(app, ui, id, style.shadow_color, |t, c| t.shadow_color = c);
    });
    if style.shadow {
        row(ui, "ระยะเงา", |ui| {
            text_slider(app, ui, id, style.shadow_offset, 0.0..=30.0, 0.5, "%", 6.0, |t, v| t.shadow_offset = v)
        });
        row(ui, "ทิศเงา", |ui| {
            text_slider(app, ui, id, style.shadow_angle, 0.0..=360.0, 5.0, "°", 45.0, |t, v| t.shadow_angle = v)
        })
        .on_hover_text("0° = ขวา, 90° = ล่าง, 180° = ซ้าย, 270° = บน");
    }
    row(ui, "พื้นหลัง", |ui| {
        let mut on = style.background;
        let resp = ui.checkbox(&mut on, "");
        text_toggle(app, id, &resp, on, |t, v| t.background = v);
        text_color(app, ui, id, style.background_color, |t, c| t.background_color = c);
    });
    if style.background {
        row(ui, "ขอบกล่อง", |ui| {
            text_slider(app, ui, id, style.box_padding, 0.0..=100.0, 1.0, "%", 30.0, |t, v| t.box_padding = v)
        });
    }

    section(ui, "ตำแหน่ง");
    row(ui, "ด่วน", |ui| {
        for (label, y) in [("บน", 0.12), ("กลาง", 0.5), ("ล่าง", 0.88)] {
            if ui.small_button(label).clicked() {
                app.edit(|tl| {
                    tl.update_clip(id, |c| {
                        let t = c.text.as_mut().unwrap();
                        t.x = 0.5;
                        t.y = y;
                    })
                });
            }
        }
    });
    row(ui, "ซ้าย-ขวา", |ui| text_slider(app, ui, id, style.x * 100.0, 0.0..=100.0, 0.1, "%", 50.0, |t, v| t.x = v / 100.0));
    row(ui, "บน-ล่าง", |ui| text_slider(app, ui, id, style.y * 100.0, 0.0..=100.0, 0.1, "%", 50.0, |t, v| t.y = v / 100.0));

    motion_editor(app, ui, id, &style.motion);
}

const DEFAULT_FONT: &str = "ค่าเริ่มต้น (Leelawadee UI)";

/// เลือกฟอนต์จากที่ติดตั้งในเครื่อง ค้นหาได้ และกรองเฉพาะฟอนต์ที่มีภาษาไทย
fn font_picker(app: &mut TadTorApp, ui: &mut egui::Ui, id: u64, style: &TextStyle) {
    if !fonts::ready() {
        ui.label(RichText::new("กำลังโหลดรายชื่อฟอนต์…").color(theme::TEXT_DIM));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        return;
    }
    let current = if style.font.is_empty() { DEFAULT_FONT } else { style.font.as_str() };
    let search_id = egui::Id::unique("font_search");
    let thai_id = egui::Id::unique("font_thai_only");
    let mut picked: Option<String> = None;
    egui::ComboBox::from_id_salt("insp_font")
        .width(ui.available_width())
        .height(380.0)
        .selected_text(current)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            let mut query: String = ui.data(|d| d.get_temp(search_id)).unwrap_or_default();
            let mut thai_only: bool = ui.data(|d| d.get_temp(thai_id)).unwrap_or(true);
            ui.add(egui::TextEdit::singleline(&mut query).hint_text("ค้นหาฟอนต์…").desired_width(f32::INFINITY));
            ui.checkbox(&mut thai_only, "เฉพาะฟอนต์ที่มีภาษาไทย");
            ui.separator();
            if ui.selectable_label(style.font.is_empty(), DEFAULT_FONT).clicked() {
                picked = Some(String::new());
            }
            let q = query.to_lowercase();
            for f in fonts::families().iter().filter(|f| (!thai_only || f.thai) && f.name.to_lowercase().contains(&q)) {
                let label = if f.thai { format!("{}  · ไทย", f.name) } else { f.name.clone() };
                if ui.selectable_label(style.font == f.name, label).clicked() {
                    picked = Some(f.name.clone());
                }
            }
            if picked.is_some() {
                ui.close();
            }
            ui.data_mut(|d| {
                d.insert_temp(search_id, query);
                d.insert_temp(thai_id, thai_only);
            });
        });
    if let Some(name) = picked {
        app.edit(|tl| tl.update_clip(id, |c| c.text.as_mut().unwrap().font = name));
    }
}

/// ตัวอย่างตัวอักษรของฟอนต์ที่เลือก (โหลดฟอนต์เข้า egui ครั้งแรกที่เห็น)
fn font_sample(ui: &mut egui::Ui, style: &TextStyle) {
    if style.font.is_empty() || !fonts::ready() {
        return;
    }
    let Some(family) = fonts::find(&style.font) else {
        ui.label(RichText::new("ไม่พบฟอนต์นี้ในเครื่อง ใช้ฟอนต์เริ่มต้นแทน").color(theme::ERROR).size(12.5));
        return;
    };
    let face = family.face(style.bold, style.italic);
    let key = format!("sample:{}", face.path.display());
    let loaded = egui::Id::unique(&key);
    if !ui.data(|d| d.get_temp::<bool>(loaded)).unwrap_or(false) {
        // ฟอนต์ใหม่ใช้ได้ตั้งแต่รอบวาดถัดไป
        if let Ok(bytes) = std::fs::read(&face.path) {
            ui.ctx().add_font(egui::epaint::text::FontInsert::new(
                &key,
                egui::FontData::from_owned(bytes),
                vec![egui::epaint::text::InsertFontFamily {
                    family: egui::FontFamily::Name(key.clone().into()),
                    priority: egui::epaint::text::FontPriority::Highest,
                }],
            ));
            ui.data_mut(|d| d.insert_temp(loaded, true));
            ui.ctx().request_repaint();
        }
        return;
    }
    let sample = if family.thai { "สวัสดี กขค Aa 123" } else { "Hello Aa 123" };
    egui::Frame::new().fill(theme::SURFACE).corner_radius(4).inner_margin(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(sample).font(egui::FontId::new(22.0, egui::FontFamily::Name(key.into()))));
        if !family.thai {
            ui.label(RichText::new("ฟอนต์นี้ไม่มีตัวอักษรไทย").color(theme::TEXT_DIM).size(12.5));
        }
    });
}

/// ภาพเคลื่อนไหวของข้อความ (ดูตัวอย่างแบบขยับได้ในแท็บ "ข้อความ")
fn motion_editor(app: &mut TadTorApp, ui: &mut egui::Ui, id: u64, m: &Motion) {
    section(ui, &format!("{} ภาพเคลื่อนไหว", icons::SPARKLE));
    fn set_motion(app: &mut TadTorApp, id: u64, f: impl FnOnce(&mut Motion)) {
        app.edit(|tl| tl.update_clip(id, |c| f(&mut c.text.as_mut().unwrap().motion)));
    }
    fn pick<T: Copy + PartialEq>(ui: &mut egui::Ui, salt: &str, cur: T, all: &[T], label: fn(T) -> &'static str) -> T {
        let mut v = cur;
        egui::ComboBox::from_id_salt(salt).width(110.0).selected_text(label(v)).show_ui(ui, |ui| {
            for &k in all {
                ui.selectable_value(&mut v, k, label(k));
            }
        });
        v
    }
    row(ui, "ตอนเข้า", |ui| {
        let v = pick(ui, "motion_in", m.anim_in, TextMotion::ALL, TextMotion::label);
        if v != m.anim_in {
            set_motion(app, id, |m| m.anim_in = v);
        }
        if m.anim_in != TextMotion::None {
            text_slider(app, ui, id, m.in_dur, 0.1..=3.0, 0.1, " วิ", 0.5, |t, v| t.motion.in_dur = v);
        }
    });
    row(ui, "ตอนออก", |ui| {
        let v = pick(ui, "motion_out", m.anim_out, TextMotion::ALL, TextMotion::label);
        if v != m.anim_out {
            set_motion(app, id, |m| m.anim_out = v);
        }
        if m.anim_out != TextMotion::None {
            text_slider(app, ui, id, m.out_dur, 0.1..=3.0, 0.1, " วิ", 0.5, |t, v| t.motion.out_dur = v);
        }
    });
    if m.anim_in != TextMotion::None || m.anim_out != TextMotion::None {
        row(ui, "จังหวะ", |ui| {
            let v = pick(ui, "motion_easing", m.easing, Easing::ALL, Easing::label);
            if v != m.easing {
                set_motion(app, id, |m| m.easing = v);
            }
        });
        row(ui, "ระยะเคลื่อน", |ui| {
            text_slider(app, ui, id, m.strength * 100.0, 0.0..=300.0, 5.0, "%", 100.0, |t, v| t.motion.strength = v / 100.0)
        })
        .on_hover_text("ระยะเลื่อน/ซูมของท่าเข้าและออก 100% = ปกติ");
    }
    row(ui, "วนซ้ำ", |ui| {
        let v = pick(ui, "motion_loop", m.looping, TextLoop::ALL, TextLoop::label);
        if v != m.looping {
            set_motion(app, id, |m| m.looping = v);
        }
    });
    if m.looping != TextLoop::None {
        row(ui, "ความเร็ววน", |ui| {
            text_slider(app, ui, id, m.loop_speed, 0.25..=3.0, 0.05, "x", 1.0, |t, v| t.motion.loop_speed = v)
        });
        row(ui, "ความแรงวน", |ui| {
            text_slider(app, ui, id, m.loop_amount * 100.0, 0.0..=300.0, 5.0, "%", 100.0, |t, v| t.motion.loop_amount = v / 100.0)
        });
    }
    ui.label(RichText::new("ดูตัวอย่างแบบขยับได้ในแท็บ \"ข้อความ\" ด้านซ้าย").color(theme::TEXT_DIM).size(12.5));
}

fn system_status(app: &mut TadTorApp, ui: &mut egui::Ui) {
    ui.label(RichText::new("ระบบ").strong());
    ui.separator();

    let status = app.ffmpeg.status();
    match &status {
        FfmpegStatus::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("กำลังตรวจ FFmpeg…");
            });
        }
        FfmpegStatus::NotFound => {
            ui.horizontal(|ui| {
                super::status_dot(ui, theme::ERROR);
                ui.label("ไม่พบ FFmpeg");
            });
            ui.label(
                RichText::new(
                    "แตกไฟล์ FFmpeg ไว้ในโฟลเดอร์ \"ffmpeg\" ข้างโปรแกรม (ให้มี ffmpeg\\bin\\ffmpeg.exe) หรือติดตั้งให้อยู่ใน PATH แล้วกดตรวจใหม่",
                )
                .color(theme::TEXT_DIM)
                .size(12.5),
            );
        }
        FfmpegStatus::Found(info) => {
            ui.horizontal(|ui| {
                super::status_dot(ui, theme::OK);
                ui.label(format!("FFmpeg {}", info.version));
            });
            ui.horizontal(|ui| {
                super::status_dot(ui, if info.ffprobe.is_some() { theme::OK } else { theme::WARN });
                ui.label(if info.ffprobe.is_some() { "ffprobe พร้อม" } else { "ไม่พบ ffprobe" });
            });
            ui.horizontal(|ui| {
                if info.nvenc_h264 {
                    super::status_dot(ui, theme::OK);
                    let extra = if info.nvenc_hevc { " + H.265" } else { "" };
                    ui.label(format!("การ์ด Nvidia พร้อม (H.264{extra})"));
                } else {
                    super::status_dot(ui, theme::WARN);
                    ui.label("ใช้ NVENC ไม่ได้ จะใช้ CPU แทน");
                }
            });
            ui.label(RichText::new(info.ffmpeg.display().to_string()).color(theme::TEXT_DIM).size(12.0)).on_hover_text("ตำแหน่ง ffmpeg ที่ใช้");
        }
    }
    ui.add_space(4.0);
    let checking = matches!(status, FfmpegStatus::Checking);
    if ui.add_enabled(!checking, egui::Button::new("ตรวจใหม่")).clicked() {
        app.ffmpeg.recheck(ui.ctx().clone());
    }

    ui.add_space(10.0);
    ui.label(RichText::new("เชื่อมต่อ AI (MCP)").strong());
    ui.separator();
    match (app.mcp.port(), &app.mcp.error) {
        (Some(port), _) => {
            ui.horizontal(|ui| {
                super::status_dot(ui, theme::OK);
                let text = if app.mcp.busy() {
                    "AI กำลังสั่งงาน…".to_owned()
                } else {
                    format!("พร้อมรับคำสั่ง (พอร์ต {port})")
                };
                ui.label(text);
            });
            dim(ui, "ให้ Claude ช่วยตัดคลิป ใส่ซับ ตัดช่วงเงียบ ฯลฯ ได้โดยตรง ตั้งค่าครั้งเดียวด้วยคำสั่งนี้ใน Terminal:");
            let cmd = crate::mcp::claude_command();
            egui::Frame::new().fill(theme::SURFACE).corner_radius(4).inner_margin(6).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(&cmd).monospace().size(12.0)).wrap());
            });
            if ui.button(format!("{} คัดลอกคำสั่ง", icons::FILE_ARROW_UP)).clicked() {
                ui.ctx().copy_text(cmd);
                app.notify(ui.ctx(), "คัดลอกแล้ว วางใน Terminal แล้วกด Enter");
            }
        }
        (None, e) => {
            ui.horizontal(|ui| {
                super::status_dot(ui, theme::WARN);
                ui.label(format!("เปิดช่องรับคำสั่งไม่ได้: {}", e.as_deref().unwrap_or("?")));
            });
        }
    }
}
