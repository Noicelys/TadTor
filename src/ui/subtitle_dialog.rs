//! ซับไตเติล: แท็บ "ซับไตเติล" (ตั้งค่า + สร้างซับอัตโนมัติด้วย whisper + แก้ซับทีละบรรทัด)
//! และหน้าต่างความคืบหน้าระหว่างถอดเสียง

use eframe::egui::{self, RichText};

use super::fader::Fader;
use super::tools_panel::{card_grid, hint, paint_text_sample, section};
use crate::app::TadTorApp;
use crate::core::fx::TextStyle;
use crate::engine::transcribe::{self, Language, TranscribeJob, TranscribeOptions, TranscribeState};
use crate::{icons, theme};

/// ช่วงเวลาที่จะถอดเสียง
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Range {
    All,
    Selected,
    FromPlayhead,
}

impl Range {
    const ALL: [Range; 3] = [Range::All, Range::Selected, Range::FromPlayhead];

    fn label(self) -> &'static str {
        match self {
            Range::All => "ทั้งงาน",
            Range::Selected => "คลิปที่เลือก",
            Range::FromPlayhead => "จากหัวเล่น",
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SubtitleSettings {
    pub language: Language,
    pub include_music: bool,
    pub range: Range,
    /// ตัวอักษรต่อบรรทัดสูงสุด (0 = ไม่จำกัด)
    pub max_len: u32,
    /// ลำดับใน `TextStyle::presets()`
    pub style: usize,
    /// คำเฉพาะที่ช่วยให้สะกดถูก
    pub prompt: String,
}

impl Default for SubtitleSettings {
    fn default() -> Self {
        let style = TextStyle::presets().iter().position(|p| p.0 == "ซับไตเติล").unwrap_or(0);
        Self { language: Language::Thai, include_music: false, range: Range::All, max_len: 42, style, prompt: String::new() }
    }
}

/// งานถอดเสียงที่สั่งไปแล้ว: ช่วงเวลาและสไตล์ที่จะใช้ตอนใส่ผลลงไทม์ไลน์
pub struct SubtitleRun {
    from: f64,
    until: f64,
    style: TextStyle,
}

/// ช่วงเวลาบนไทม์ไลน์ตามที่ตั้งไว้ (None = ยังไม่ได้เลือกคลิป / ช่วงว่าง)
fn range_span(app: &TadTorApp, range: Range) -> Option<(f64, f64)> {
    let total = app.timeline.duration();
    let (a, b) = match range {
        Range::All => (0.0, total),
        Range::FromPlayhead => (app.playhead, total),
        Range::Selected => {
            let c = app.selected_clip.and_then(|id| app.timeline.clip(id))?;
            (c.start, c.end())
        }
    };
    (b - a > 0.1).then_some((a, b))
}

fn mmss(t: f64) -> String {
    let t = t.max(0.0);
    format!("{}:{:04.1}", (t / 60.0).floor() as u64, t % 60.0)
}

fn start(app: &mut TadTorApp, ctx: &egui::Context) {
    if app.timeline.main.is_empty() && app.timeline.overlay.is_empty() && app.timeline.audio.is_empty() {
        app.notify(ctx, "ยังไม่มีคลิปให้ถอดเสียง");
        return;
    }
    let s = &app.settings.subtitles;
    let Some((from, until)) = range_span(app, s.range) else {
        let msg = if s.range == Range::Selected {
            "คลิกเลือกคลิปในไทม์ไลน์ก่อน"
        } else {
            "ช่วงที่เลือกสั้นเกินไป"
        };
        app.notify(ctx, msg);
        return;
    };
    let opts = TranscribeOptions {
        language: s.language,
        include_music: s.include_music,
        from,
        until,
        max_len: s.max_len,
        prompt: s.prompt.clone(),
    };
    let presets = TextStyle::presets();
    let style = presets.get(s.style).map_or_else(|| TextStyle::subtitle(String::new()), |p| p.1.clone());
    if let Err(e) = start_job(app, ctx, opts, style) {
        app.notify(ctx, e);
    }
}

/// เริ่มถอดเสียง (ใช้ทั้งปุ่มในแท็บและคำสั่งจาก AI) ผลจะใส่ลงไทม์ไลน์เมื่อเสร็จ
pub fn start_job(app: &mut TadTorApp, ctx: &egui::Context, opts: TranscribeOptions, style: TextStyle) -> Result<(), String> {
    if app.transcribe_job.is_some() {
        return Err("กำลังสร้างซับอยู่แล้ว".to_owned());
    }
    if opts.until - opts.from <= 0.1 {
        return Err("ช่วงที่จะถอดสั้นเกินไป".to_owned());
    }
    let info = app.ffmpeg.info().ok_or("ไม่พบ FFmpeg")?;
    app.stop_playback();
    app.subtitle_dialog = Some(SubtitleRun { from: opts.from, until: opts.until, style });
    app.transcribe_job = Some(TranscribeJob::start(info.ffmpeg, &app.timeline, opts, ctx.clone()));
    Ok(())
}

/// ใส่ผลถอดเสียงลงไทม์ไลน์ (ครั้งเดียวต่อการถอด)
pub fn apply_result(app: &mut TadTorApp, items: &[(f64, f64, String)]) {
    if let Some(run) = app.subtitle_dialog.take() {
        if !items.is_empty() {
            let items = items.to_vec();
            app.edit(|tl| tl.replace_subtitles_in(run.from, run.until, items, &run.style));
        }
    }
}

// ---------------- แท็บซับไตเติล ----------------

pub fn tab(app: &mut TadTorApp, ui: &mut egui::Ui) {
    auto_section(app, ui);
    ui.add_space(8.0);
    edit_section(app, ui);
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([108.0, 26.0], egui::Label::new(RichText::new(label).color(theme::TEXT_DIM)));
        add(ui);
    });
}

fn auto_section(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    section(ui, &format!("{}  สร้างซับอัตโนมัติ", icons::MAGIC_WAND));
    hint(ui, "ฟังเสียงพูดแล้วพิมพ์เป็นซับให้ ทำงานในเครื่องนี้ทั้งหมด ไม่ส่งไฟล์ขึ้นอินเทอร์เน็ต");
    ui.add_space(4.0);

    let span = range_span(app, app.settings.subtitles.range);
    let s = &mut app.settings.subtitles;
    row(ui, "ภาษาที่พูด", |ui| {
        egui::ComboBox::from_id_salt("sub_lang").selected_text(s.language.label()).show_ui(ui, |ui| {
            for l in Language::ALL {
                ui.selectable_value(&mut s.language, l, l.label());
            }
        });
    });
    row(ui, "ช่วงที่ถอด", |ui| {
        for r in Range::ALL {
            ui.selectable_value(&mut s.range, r, r.label());
        }
    });
    row(ui, "", |ui| {
        let text = match span {
            Some((a, b)) => format!("{} – {} (ยาว {:.1} วิ)", mmss(a), mmss(b), b - a),
            None if s.range == Range::Selected => "ยังไม่ได้เลือกคลิป".to_owned(),
            None => "ช่วงว่าง".to_owned(),
        };
        ui.label(RichText::new(text).color(if span.is_some() { theme::TEXT_DIM } else { theme::WARN }).size(13.0));
    });
    row(ui, "ตัวอักษร/บรรทัด", |ui| {
        ui.add(Fader::new(&mut s.max_len, 0..=80).default_value(42));
    });
    row(ui, "", |ui| {
        let text = if s.max_len == 0 {
            "ไม่จำกัด (ซับยาวตามประโยค)".to_owned()
        } else {
            format!("ซับยาวไม่เกิน ~{} ตัวอักษร", s.max_len)
        };
        ui.label(RichText::new(text).color(theme::TEXT_DIM).size(13.0));
    });
    row(ui, "คำเฉพาะ", |ui| {
        ui.add(egui::TextEdit::singleline(&mut s.prompt).hint_text("ชื่อคน/แบรนด์/ศัพท์ที่พูดถึง เช่น TadTor, ไทม์ไลน์").desired_width(f32::INFINITY))
            .on_hover_text("ช่วยให้สะกดคำเหล่านี้ถูก คั่นด้วยช่องว่างหรือจุลภาค (เว้นว่างได้)");
    });
    ui.checkbox(&mut s.include_music, "ถอดเสียงจากแทร็กเสียงด้วย (ปกติไม่ต้อง เพลงทำให้ถอดพลาด)");

    ui.add_space(4.0);
    ui.label(RichText::new("สไตล์ของซับที่สร้าง").strong());
    let presets = TextStyle::presets();
    let idx: Vec<usize> = (0..presets.len()).collect();
    if let Some(i) =
        card_grid(ui, &idx, Some(s.style), |i| presets[i].0.to_owned(), |p, rect, i, time| paint_text_sample(p, rect, &presets[i].1, time))
    {
        s.style = i;
    }

    if !transcribe::ready() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(icons::CLOUD_ARROW_DOWN).color(theme::WARN));
            ui.label(
                RichText::new(format!("ครั้งแรกต้องดาวน์โหลดโปรแกรมถอดเสียงและโมเดลภาษา ราว {} MB (ครั้งเดียว)", transcribe::DOWNLOAD_MB))
                    .color(theme::WARN)
                    .size(13.0),
            );
        });
    }
    if let Some((a, b)) = span {
        if app.timeline.subtitles.iter().any(|c| c.end() > a + 1e-6 && c.start < b - 1e-6) {
            hint(ui, "ซับเดิมในช่วงนี้จะถูกแทนที่ (กด Ctrl+Z ย้อนได้)");
        }
    }
    ui.add_space(4.0);
    let go = egui::Button::new(RichText::new(format!("{}  เริ่มสร้างซับ", icons::MAGIC_WAND)).color(egui::Color32::BLACK).strong())
        .fill(theme::ACCENT)
        .min_size(egui::vec2(ui.available_width(), 34.0));
    if ui.add_enabled(app.transcribe_job.is_none(), go).clicked() {
        start(app, &ctx);
    }
}

fn edit_section(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let n = app.timeline.subtitles.len();
    section(ui, &format!("{}  แก้ซับ ({n} บรรทัด)", icons::SUBTITLES));
    ui.horizontal(|ui| {
        let half = egui::vec2((ui.available_width() - 8.0) / 2.0, 28.0);
        if ui.add(egui::Button::new(format!("{} นำเข้า .srt", icons::FILE_ARROW_DOWN)).min_size(half)).clicked() {
            app.import_srt(&ctx);
        }
        if ui.add_enabled(n > 0, egui::Button::new(format!("{} ส่งออก .srt", icons::FILE_ARROW_UP)).min_size(half)).clicked() {
            app.export_srt(&ctx);
        }
    });
    if n == 0 {
        hint(ui, "ยังไม่มีซับ สร้างอัตโนมัติด้านบน หรือนำเข้าไฟล์ .srt");
        return;
    }

    row(ui, "เลื่อนเวลาทั้งหมด", |ui| {
        for dt in [-0.5, -0.1, 0.1, 0.5] {
            let label = if dt < 0.0 { format!("{dt}") } else { format!("+{dt}") };
            if ui.small_button(label).on_hover_text("วินาที (ซับขึ้นช้า/เร็วกว่าเสียง)").clicked()
            {
                app.edit(|tl| tl.shift_subtitles(dt));
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button(format!("{} ใช้สไตล์ซับที่เลือกกับทุกบรรทัด", icons::TEXT_AA)).clicked()
        {
            let style = app.selected_clip.and_then(|id| app.timeline.subtitles.iter().find(|c| c.id == id)).and_then(|c| c.text.clone());
            match style {
                Some(style) => app.edit(|tl| {
                    for c in &mut tl.subtitles {
                        if let Some(t) = &mut c.text {
                            *t = TextStyle { content: std::mem::take(&mut t.content), ..style.clone() };
                        }
                    }
                }),
                None => app.notify(&ctx, "คลิกเลือกซับบรรทัดที่แต่งไว้ก่อน"),
            }
        }
        if ui.small_button(format!("{} ลบทั้งหมด", icons::TRASH)).clicked() {
            app.edit(|tl| tl.subtitles.clear());
        }
    });
    hint(ui, "คลิกเวลาเพื่อไปที่บรรทัดนั้น • แก้คำได้เลย • แต่งสไตล์ทีละบรรทัดที่แผงขวา");
    ui.add_space(4.0);

    let rows: Vec<(u64, f64, f64, String)> =
        app.timeline.subtitles.iter().map(|c| (c.id, c.start, c.end(), c.title().to_owned())).collect();
    let mut remove = None;
    for (id, s, e, text) in rows {
        let selected = app.selected_clip == Some(id);
        let playing_here = app.playhead >= s && app.playhead < e;
        let fill = if selected { theme::SURFACE_HOVER } else { theme::SURFACE };
        egui::Frame::new().fill(fill).corner_radius(4).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let color = if playing_here { theme::ACCENT } else { theme::TEXT_DIM };
                let time = egui::Button::new(RichText::new(mmss(s)).color(color).size(13.0).monospace()).frame(false);
                if ui.add(time).on_hover_text(format!("{} – {}", mmss(s), mmss(e))).clicked() {
                    app.seek(s);
                    app.selected_clip = Some(id);
                }
                let mut t = text;
                let edit = egui::TextEdit::singleline(&mut t).id_salt(("sub_line", id)).desired_width(ui.available_width() - 30.0);
                let resp = ui.add(edit);
                if resp.gained_focus() {
                    app.selected_clip = Some(id);
                }
                app.live_edit(&resp, |tl| {
                    tl.update_clip(id, |c| {
                        if let Some(st) = c.text.as_mut() {
                            st.content = t;
                        }
                    })
                });
                if ui.small_button(icons::TRASH).on_hover_text("ลบบรรทัดนี้").clicked() {
                    remove = Some(id);
                }
            });
        });
        ui.add_space(2.0);
    }
    if let Some(id) = remove {
        app.edit(|tl| tl.remove(id));
        if app.selected_clip == Some(id) {
            app.selected_clip = None;
        }
    }
}

// ---------------- หน้าต่างความคืบหน้า ----------------

pub fn show(app: &mut TadTorApp, ctx: &egui::Context) {
    if app.transcribe_job.is_some() {
        progress(app, ctx);
    }
}

fn progress(app: &mut TadTorApp, ctx: &egui::Context) {
    let Some(job) = &app.transcribe_job else { return };
    let state = job.state();
    let mut cancel = false;
    let mut close = false;
    egui::Modal::new(egui::Id::unique("subtitle_progress")).show(ctx, |ui| {
        ui.set_width(400.0);
        let bar = |ui: &mut egui::Ui, title: &str, p: Option<f32>| {
            ui.label(RichText::new(title).strong().size(15.0));
            ui.add_space(8.0);
            match p {
                Some(p) => ui.add(egui::ProgressBar::new(p).show_percentage().animate(true)),
                None => ui.add(egui::ProgressBar::new(0.0).animate(true)),
            };
        };
        match &state {
            TranscribeState::Downloading { what, progress } => {
                bar(ui, &format!("กำลังดาวน์โหลด{what}…"), Some(*progress));
            }
            TranscribeState::Rendering => bar(ui, "กำลังเตรียมเสียง…", None),
            TranscribeState::Running { progress } => {
                bar(ui, "กำลังถอดเสียง…", Some(*progress));
                ui.label(RichText::new("ใช้เวลาประมาณเท่ากับความยาวช่วงที่ถอด (ใช้ CPU)").color(theme::TEXT_DIM).size(13.0));
            }
            TranscribeState::Done(items) => {
                ui.horizontal(|ui| {
                    super::status_dot(ui, theme::OK);
                    let msg = if items.is_empty() {
                        "ไม่พบเสียงพูด".to_owned()
                    } else {
                        format!("ได้ซับไตเติล {} บรรทัด", items.len())
                    };
                    ui.label(RichText::new(msg).strong().size(15.0));
                });
                ui.label(RichText::new("ตรวจคำผิดได้ในแท็บ \"ซับไตเติล\" หรือคลิกซับในไทม์ไลน์").color(theme::TEXT_DIM).size(13.0));
                ui.add_space(8.0);
                if ui.button("ปิด").clicked() {
                    close = true;
                }
            }
            TranscribeState::Failed(e) => {
                ui.horizontal(|ui| {
                    super::status_dot(ui, theme::ERROR);
                    ui.label(RichText::new("สร้างซับไตเติลไม่สำเร็จ").strong().size(15.0));
                });
                ui.add(egui::Label::new(RichText::new(e).color(theme::ERROR).size(13.0)).wrap());
                ui.add_space(8.0);
                if ui.button("ปิด").clicked() {
                    close = true;
                }
            }
            TranscribeState::Cancelled => {
                ui.label("ยกเลิกแล้ว");
                if ui.button("ปิด").clicked() {
                    close = true;
                }
            }
        }
        let running = matches!(state, TranscribeState::Downloading { .. } | TranscribeState::Rendering | TranscribeState::Running { .. });
        if running {
            ui.add_space(8.0);
            if ui.button("ยกเลิก").clicked() {
                cancel = true;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    });

    if cancel {
        job.cancel();
    }
    // เสร็จแล้วใส่ซับลงไทม์ไลน์ทันที (ครั้งเดียว)
    if let TranscribeState::Done(items) = &state {
        apply_result(app, items);
    }
    if close {
        app.transcribe_job = None;
        app.subtitle_dialog = None;
    }
}
