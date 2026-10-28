//! หน้าต่างส่งออกวิดีโอ: ตั้งค่า → แถบความคืบหน้า → เสร็จ

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};

use crate::app::TadTorApp;
use crate::engine::export::{Codec, Encoder, ExportJob, ExportSettings, ExportState, Quality, Speed};
use crate::ffmpeg::graph::VideoSpec;
use crate::{icons, theme};

pub struct ExportDialog {
    pub path: PathBuf,
    pub short_side: u32,
    pub fps: u32,
    pub encoder: Encoder,
    pub codec: Codec,
    pub quality: Quality,
    pub crf: u32,
    pub speed: Speed,
    pub audio_bitrate: u32,
    nvenc: bool,
    nvenc_hevc: bool,
}

impl ExportDialog {
    pub fn new(app: &TadTorApp) -> Self {
        let dir = app
            .project_path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Videos")))
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let nvenc = app.ffmpeg.info().is_some_and(|i| i.nvenc_h264);
        let nvenc_hevc = app.ffmpeg.info().is_some_and(|i| i.nvenc_hevc);
        Self {
            path: unique_path(&dir, &safe_name(&app.project.name)),
            short_side: app.project.quality,
            fps: app.project.fps,
            encoder: if nvenc { Encoder::Nvenc } else { Encoder::Cpu },
            codec: Codec::H264,
            quality: Quality::High,
            crf: 20,
            speed: Speed::Balanced,
            audio_bitrate: 192,
            nvenc,
            nvenc_hevc,
        }
    }
}

/// ตัดอักขระที่ใช้ตั้งชื่อไฟล์ใน Windows ไม่ได้
fn safe_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c }).collect();
    let s = s.trim().to_owned();
    if s.is_empty() {
        "วิดีโอ".to_owned()
    } else {
        s
    }
}

/// ไม่ทับไฟล์เดิม: ถ้ามีชื่อนี้แล้วเติม (2), (3), …
fn unique_path(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.mp4"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{stem} ({n}).mp4"))).find(|p| !p.exists()).unwrap()
}

pub fn show(app: &mut TadTorApp, ctx: &egui::Context) {
    if app.export_job.is_some() {
        progress(app, ctx);
    } else if app.export_dialog.is_some() {
        settings(app, ctx);
    }
}

fn settings(app: &mut TadTorApp, ctx: &egui::Context) {
    let aspect = app.project.aspect;
    let mut start = false;
    let mut close = false;
    let Some(dlg) = app.export_dialog.as_mut() else { return };

    egui::Modal::new(egui::Id::unique("export_settings")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.label(RichText::new("ส่งออกวิดีโอ").strong().size(16.0));
        ui.add_space(8.0);

        let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            ui.horizontal(|ui| {
                ui.add_sized([104.0, 26.0], egui::Label::new(RichText::new(label).color(theme::TEXT_DIM)));
                add(ui);
            });
        };

        row(ui, "บันทึกที่", &mut |ui| {
            let shown = super::ellipsize(&dlg.path.display().to_string(), 36);
            ui.label(RichText::new(shown).size(13.0)).on_hover_text(dlg.path.display().to_string());
            if ui.button("เลือก…").clicked() {
                let mut d = rfd::FileDialog::new().set_title("บันทึกวิดีโอ").add_filter("MP4", &["mp4"]);
                if let Some(dir) = dlg.path.parent() {
                    d = d.set_directory(dir);
                }
                if let Some(name) = dlg.path.file_name() {
                    d = d.set_file_name(name.to_string_lossy());
                }
                if let Some(mut p) = d.save_file() {
                    if p.extension().is_none_or(|e| !e.eq_ignore_ascii_case("mp4")) {
                        p.set_extension("mp4");
                    }
                    dlg.path = p;
                }
            }
        });
        row(ui, "ความละเอียด", &mut |ui| {
            let label = |q: u32| {
                let (w, h) = aspect.resolution(q);
                let name = match q {
                    480 => "480p",
                    720 => "720p",
                    1080 => "1080p",
                    1440 => "2K",
                    2160 => "4K",
                    _ => "",
                };
                format!("{name}  ({w}×{h})")
            };
            egui::ComboBox::from_id_salt("exp_res").selected_text(label(dlg.short_side)).width(200.0).show_ui(ui, |ui| {
                for q in [480, 720, 1080, 1440, 2160] {
                    ui.selectable_value(&mut dlg.short_side, q, label(q));
                }
            });
        });
        row(ui, "เฟรมเรต", &mut |ui| {
            egui::ComboBox::from_id_salt("exp_fps").selected_text(format!("{} fps", dlg.fps)).show_ui(ui, |ui| {
                for f in [15, 24, 25, 30, 48, 50, 60] {
                    ui.selectable_value(&mut dlg.fps, f, format!("{f} fps"));
                }
            });
        });
        row(ui, "คุณภาพ", &mut |ui| {
            for q in Quality::ALL {
                ui.selectable_value(&mut dlg.quality, q, q.label());
            }
        });
        if dlg.quality == Quality::Custom {
            row(ui, "", &mut |ui| {
                ui.label("CRF");
                ui.add(super::fader::Fader::new(&mut dlg.crf, 12..=40).default_value(20));
            });
            row(ui, "", &mut |ui| {
                ui.label(RichText::new("ค่าน้อย = ชัดมาก ไฟล์ใหญ่ • 18–23 คือช่วงปกติ").color(theme::TEXT_DIM).size(12.5));
            });
        }
        row(ui, "ตัวเข้ารหัส", &mut |ui| {
            ui.add_enabled_ui(dlg.nvenc, |ui| {
                ui.selectable_value(&mut dlg.encoder, Encoder::Nvenc, "การ์ดจอ Nvidia (เร็ว)").on_disabled_hover_text("เครื่องนี้ใช้ NVENC ไม่ได้");
            });
            ui.selectable_value(&mut dlg.encoder, Encoder::Cpu, "CPU");
        });
        row(ui, "รูปแบบ", &mut |ui| {
            for c in Codec::ALL {
                let ok = c == Codec::H264 || dlg.encoder == Encoder::Cpu || dlg.nvenc_hevc;
                ui.add_enabled_ui(ok, |ui| {
                    ui.selectable_value(&mut dlg.codec, c, c.label()).on_disabled_hover_text("การ์ดจอนี้ทำ H.265 ไม่ได้ เลือก CPU แทน");
                });
            }
        });
        if dlg.codec == Codec::H265 && dlg.encoder == Encoder::Nvenc && !dlg.nvenc_hevc {
            dlg.codec = Codec::H264;
        }
        row(ui, "ความเร็ว", &mut |ui| {
            for sp in Speed::ALL {
                ui.selectable_value(&mut dlg.speed, sp, sp.label());
            }
        });
        row(ui, "เสียง", &mut |ui| {
            egui::ComboBox::from_id_salt("exp_abr").selected_text(format!("AAC {} kbps", dlg.audio_bitrate)).show_ui(ui, |ui| {
                for b in [96, 128, 192, 256, 320] {
                    ui.selectable_value(&mut dlg.audio_bitrate, b, format!("{b} kbps"));
                }
            });
        });

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let go = egui::Button::new(RichText::new(format!("{}  เริ่มส่งออก", icons::EXPORT)).color(egui::Color32::BLACK).strong())
                .fill(theme::ACCENT)
                .min_size(egui::vec2(120.0, 30.0));
            if ui.add(go).clicked() {
                start = true;
            }
            if ui.button("ยกเลิก").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
        });
    });

    if close {
        app.export_dialog = None;
    }
    if start {
        let Some(dlg) = app.export_dialog.take() else { return };
        let Some(info) = app.ffmpeg.info() else {
            app.notify(ctx, "ไม่พบ FFmpeg");
            return;
        };
        let (width, height) = aspect.resolution(dlg.short_side);
        let settings = ExportSettings {
            path: dlg.path,
            video: VideoSpec { width, height, fps: dlg.fps },
            encoder: dlg.encoder,
            codec: dlg.codec,
            quality: dlg.quality,
            crf: dlg.crf,
            speed: dlg.speed,
            audio_bitrate: dlg.audio_bitrate,
        };
        app.export_job = Some(ExportJob::start(&info.ffmpeg, &app.timeline, settings, ctx.clone()));
    }
}

fn progress(app: &mut TadTorApp, ctx: &egui::Context) {
    let Some(job) = &app.export_job else { return };
    let state = job.state();
    let mut close = false;
    let mut cancel = false;

    egui::Modal::new(egui::Id::unique("export_progress")).show(ctx, |ui| {
        ui.set_width(400.0);
        match &state {
            ExportState::Running { progress, started } => {
                ui.label(RichText::new("กำลังส่งออก…").strong().size(15.0));
                ui.add_space(8.0);
                ui.add(egui::ProgressBar::new(*progress).show_percentage().animate(true));
                let elapsed = started.elapsed().as_secs_f64();
                let remain = if *progress > 0.02 { elapsed / *progress as f64 * (1.0 - *progress as f64) } else { f64::NAN };
                let text = if remain.is_finite() {
                    format!("ใช้ไป {} • เหลืออีกประมาณ {}", mmss(elapsed), mmss(remain))
                } else {
                    format!("ใช้ไป {}", mmss(elapsed))
                };
                ui.label(RichText::new(text).color(theme::TEXT_DIM));
                ui.add_space(8.0);
                if ui.button("ยกเลิกการส่งออก").clicked() {
                    cancel = true;
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
            ExportState::Done { path, seconds } => {
                ui.horizontal(|ui| {
                    super::status_dot(ui, theme::OK);
                    ui.label(RichText::new("ส่งออกเสร็จแล้ว").strong().size(15.0));
                });
                ui.add_space(4.0);
                let size = std::fs::metadata(path).map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0);
                ui.label(RichText::new(format!("ใช้เวลา {} • ขนาด {size:.1} MB", mmss(*seconds))).color(theme::TEXT_DIM));
                ui.add(egui::Label::new(RichText::new(path.display().to_string()).size(12.5)).wrap());
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("{} เปิดโฟลเดอร์", icons::FOLDER_OPEN)).clicked() {
                        reveal(path);
                    }
                    if ui.button("ปิด").clicked() {
                        close = true;
                    }
                });
            }
            ExportState::Failed(e) => {
                ui.horizontal(|ui| {
                    super::status_dot(ui, theme::ERROR);
                    ui.label(RichText::new("ส่งออกไม่สำเร็จ").strong().size(15.0));
                });
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(e).color(theme::ERROR).size(13.0)).wrap());
                ui.add_space(10.0);
                if ui.button("ปิด").clicked() {
                    close = true;
                }
            }
            ExportState::Cancelled => {
                ui.label(RichText::new("ยกเลิกการส่งออกแล้ว").strong());
                ui.add_space(10.0);
                if ui.button("ปิด").clicked() {
                    close = true;
                }
            }
        }
    });

    if cancel {
        job.cancel();
    }
    if close {
        app.export_job = None;
    }
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// เปิด File Explorer แล้วเลือกไฟล์ไว้
fn reveal(path: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
}
