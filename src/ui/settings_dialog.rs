//! หน้าต่างการตั้งค่าโปรแกรม (เมนู > การตั้งค่า หรือ Ctrl+,) และหน้าต่างกู้คืนงานที่ยังไม่ได้บันทึก

use eframe::egui::{self, RichText};

use super::fader::Fader;
use super::layout::{Layout, ZOOMS};
use crate::app::TadTorApp;
use crate::core::project::AspectRatio;
use crate::settings::{self, PreviewQuality, Settings};
use crate::{icons, theme};

pub fn show(app: &mut TadTorApp, ctx: &egui::Context) {
    if app.recovery_prompt {
        recovery(app, ctx);
    } else if app.settings_open {
        window(app, ctx);
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).strong());
    ui.separator();
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 26.0], egui::Label::new(RichText::new(label).color(theme::TEXT_DIM)));
        add(ui);
    });
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::TEXT_DIM).size(12.5));
}

fn quality_label(q: u32) -> &'static str {
    match q {
        480 => "480p",
        720 => "720p",
        1080 => "1080p",
        1440 => "2K",
        2160 => "4K",
        _ => "อื่น ๆ",
    }
}

fn window(app: &mut TadTorApp, ctx: &egui::Context) {
    let mut close = false;
    egui::Modal::new(egui::Id::unique("settings_window")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}  การตั้งค่า", icons::SLIDERS_HORIZONTAL)).strong().size(17.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(icons::X_CIRCLE).on_hover_text("ปิด (Esc)").clicked() {
                    close = true;
                }
            });
        });
        egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.75).show(ui, |ui| {
            let s = &mut app.settings;

            section(ui, "การบันทึกงาน");
            ui.checkbox(&mut s.open_last_project, "เปิดงานล่าสุดให้เองตอนเปิดโปรแกรม");
            ui.checkbox(&mut s.autosave, "บันทึกอัตโนมัติ");
            if s.autosave {
                row(ui, "บันทึกทุก", |ui| {
                    ui.add(Fader::new(&mut s.autosave_minutes, 1..=30).suffix(" นาที").default_value(3));
                });
                hint(ui, "งานที่บันทึกแล้วจะบันทึกทับไฟล์เดิม • งานที่ยังไม่เคยบันทึกจะเก็บสำรองไว้ ถ้าโปรแกรมปิดกะทันหันจะถามให้กู้คืนตอนเปิดครั้งหน้า");
            }
            ui.horizontal(|ui| {
                hint(ui, &format!("งานล่าสุด {} งาน", s.recent.len()));
                if ui.add_enabled(!s.recent.is_empty(), egui::Button::new("ล้างรายการ").small()).clicked() {
                    s.recent.clear();
                }
            });

            section(ui, "โปรเจกต์ใหม่");
            let np = &mut s.new_project;
            row(ui, "อัตราส่วน", |ui| {
                egui::ComboBox::from_id_salt("set_aspect").selected_text(np.aspect.label()).show_ui(ui, |ui| {
                    for a in AspectRatio::ALL {
                        ui.selectable_value(&mut np.aspect, a, a.label());
                    }
                });
            });
            row(ui, "ความละเอียด", |ui| {
                egui::ComboBox::from_id_salt("set_quality").selected_text(quality_label(np.quality)).show_ui(ui, |ui| {
                    for q in [720, 1080, 1440, 2160] {
                        ui.selectable_value(&mut np.quality, q, quality_label(q));
                    }
                });
            });
            row(ui, "เฟรมเรต", |ui| {
                egui::ComboBox::from_id_salt("set_fps").selected_text(format!("{} fps", np.fps)).show_ui(ui, |ui| {
                    for f in [24, 25, 30, 50, 60] {
                        ui.selectable_value(&mut np.fps, f, format!("{f} fps"));
                    }
                });
            });
            hint(ui, "ใช้กับโปรเจกต์ที่สร้างใหม่ (Ctrl+N) • โปรเจกต์ที่เปิดอยู่แก้ได้ที่การ์ดรายละเอียด");

            section(ui, "พรีวิว");
            row(ui, "คุณภาพภาพพรีวิว", |ui| {
                for q in PreviewQuality::ALL {
                    ui.selectable_value(&mut s.preview, q, q.label());
                }
            });
            hint(ui, "ลื่น = เครื่องช้าก็เล่นไม่กระตุก • ไม่มีผลกับไฟล์ที่ส่งออก");

            section(ui, "ส่งออก");
            row(ui, "โฟลเดอร์เริ่มต้น", |ui| {
                let text = match &s.export_dir {
                    Some(d) => super::ellipsize(&d.display().to_string(), 34),
                    None => "ข้างไฟล์โปรเจกต์ (หรือ Videos)".to_owned(),
                };
                ui.label(text);
            });
            ui.horizontal(|ui| {
                ui.add_space(158.0);
                if ui.button(format!("{} เลือก…", icons::FOLDER_OPEN)).clicked() {
                    if let Some(d) = rfd::FileDialog::new().set_title("โฟลเดอร์ส่งออกเริ่มต้น").pick_folder()
                    {
                        s.export_dir = Some(d);
                    }
                }
                if s.export_dir.is_some() && ui.button("ใช้ข้างไฟล์โปรเจกต์").clicked() {
                    s.export_dir = None;
                }
            });

            section(ui, "หน้าจอ");
            row(ui, "ขนาดตัวอักษร", |ui| {
                for z in ZOOMS {
                    let on = (app.layout.ui_zoom - z).abs() < 0.01;
                    if ui.add(egui::Button::selectable(on, format!("{:.0}%", z * 100.0))).clicked() {
                        ctx.set_zoom_factor(z);
                    }
                }
            });
            row(ui, "จัดการ์ด", |ui| {
                for (name, preset) in Layout::presets() {
                    let on = app.layout.row == preset.row;
                    if ui.add(egui::Button::selectable(on, name.split(' ').next().unwrap_or(name))).on_hover_text(name).clicked() {
                        app.layout.row = preset.row;
                        super::layout::save(&app.layout);
                    }
                }
            });
            let mut top = app.layout.timeline_top;
            if ui.checkbox(&mut top, "ไทม์ไลน์ไว้ด้านบน").changed() {
                app.layout.timeline_top = top;
                super::layout::save(&app.layout);
            }

            section(ui, "ไฟล์ตั้งค่า");
            ui.horizontal(|ui| {
                hint(ui, &settings::dir().display().to_string());
                if ui.small_button(format!("{} เปิดโฟลเดอร์", icons::FOLDER_OPEN)).clicked() {
                    let _ = std::process::Command::new("explorer").arg(settings::dir()).spawn();
                }
            });
            if ui.button(format!("{} คืนค่าเริ่มต้นทั้งหมด", icons::ARROW_COUNTER_CLOCKWISE)).on_hover_text("ไม่ลบรายการงานล่าสุด").clicked()
            {
                let recent = std::mem::take(&mut app.settings.recent);
                app.settings = Settings { recent, ..Settings::default() };
                app.layout = Layout::default();
                super::layout::save(&app.layout);
                ctx.set_zoom_factor(1.0);
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
    });
    if close {
        app.settings_open = false;
    }
}

/// ครั้งก่อนโปรแกรมปิดกะทันหันขณะมีงานที่ยังไม่เคยบันทึก
fn recovery(app: &mut TadTorApp, ctx: &egui::Context) {
    let mut choice = None;
    egui::Modal::new(egui::Id::unique("recovery")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(RichText::new("พบงานที่ยังไม่ได้บันทึก").strong().size(15.0));
        ui.add_space(4.0);
        ui.label(RichText::new("ครั้งก่อนโปรแกรมปิดไปก่อนบันทึกงาน จะกู้คืนงานนั้นไหม?").color(theme::TEXT_DIM));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let restore = egui::Button::new(RichText::new("กู้คืนงาน").color(egui::Color32::BLACK).strong()).fill(theme::ACCENT);
            if ui.add(restore).clicked() {
                choice = Some(true);
            }
            if ui.button("ทิ้ง").clicked() {
                choice = Some(false);
            }
        });
    });
    match choice {
        Some(true) => {
            app.recovery_prompt = false;
            app.restore_recovery(ctx);
        }
        Some(false) => {
            app.recovery_prompt = false;
            let _ = std::fs::remove_file(settings::recovery_file());
        }
        None => {}
    }
}
