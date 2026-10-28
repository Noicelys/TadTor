use eframe::egui::{self, RichText};

use crate::app::{PendingAction, TadTorApp};
use crate::{icons, theme};

pub fn show(app: &mut TadTorApp, ui: &mut egui::Ui) {
    egui::Panel::top("top_bar")
        .exact_size(44.0)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::symmetric(12, 0)))
        .show(ui, |ui| {
            let ctx = ui.ctx().clone();
            ui.horizontal_centered(|ui| {
                // โลโก้
                if let Some(logo) = &app.logo {
                    ui.add(egui::Image::new((logo.id(), egui::vec2(26.0, 26.0))));
                }
                ui.label(RichText::new("TadTor").strong().size(16.0));
                ui.add_space(12.0);

                ui.menu_button(format!("{} เมนู", icons::LIST), |ui| {
                    let item = |ui: &mut egui::Ui, text: &str, key: &str| ui.add(egui::Button::new(text).shortcut_text(key)).clicked();
                    if item(ui, &format!("{}  โปรเจกต์ใหม่", icons::FILE_PLUS), "Ctrl+N") {
                        ui.close();
                        app.request(&ctx, PendingAction::NewProject);
                    }
                    if item(ui, &format!("{}  เปิดโปรเจกต์…", icons::FOLDER_OPEN), "Ctrl+O") {
                        ui.close();
                        app.request(&ctx, PendingAction::OpenProject);
                    }
                    let recent: Vec<_> = app.settings.recent.iter().filter(|p| p.is_file()).cloned().collect();
                    ui.add_enabled_ui(!recent.is_empty(), |ui| {
                        ui.menu_button(format!("{}  เปิดงานล่าสุด", icons::FOLDER_OPEN), |ui| {
                            for p in recent {
                                let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                                if ui.button(&name).on_hover_text(p.display().to_string()).clicked() {
                                    ui.close();
                                    app.request(&ctx, PendingAction::OpenPath(p));
                                }
                            }
                        });
                    });
                    if item(ui, &format!("{}  บันทึก", icons::FLOPPY_DISK), "Ctrl+S") {
                        ui.close();
                        app.save(&ctx, false);
                    }
                    if item(ui, &format!("{}  บันทึกเป็น…", icons::FLOPPY_DISK), "Ctrl+Shift+S") {
                        ui.close();
                        app.save(&ctx, true);
                    }
                    ui.separator();
                    if item(ui, &format!("{}  นำเข้าไฟล์สื่อ…", icons::FOLDER_SIMPLE_PLUS), "Ctrl+I") {
                        ui.close();
                        app.import_dialog(&ctx);
                    }
                    if item(ui, &format!("{}  ส่งออกวิดีโอ…", icons::EXPORT), "Ctrl+E") {
                        ui.close();
                        app.open_export_dialog(&ctx);
                    }
                    ui.separator();
                    if item(ui, &format!("{}  การตั้งค่า…", icons::SLIDERS_HORIZONTAL), "Ctrl+,") {
                        ui.close();
                        app.settings_open = true;
                    }
                    ui.menu_button(format!("{}  จัดหน้าจอ", icons::SLIDERS_HORIZONTAL), |ui| {
                        super::layout::menu(app, ui)
                    });
                    if item(ui, &format!("{}  สร้างทางลัดบนเดสก์ท็อป", icons::FILE_PLUS), "") {
                        ui.close();
                        match crate::paths::create_shortcuts() {
                            Ok(_) => app.notify(
                                &ctx,
                                format!("สร้างทางลัดบนเดสก์ท็อปและเมนู Start แล้ว • เปิดโปรแกรมได้ด้วย {}", crate::paths::HOTKEY_LABEL),
                            ),
                            Err(e) => app.notify(&ctx, e),
                        }
                    }
                    ui.separator();
                    if ui.button(format!("{}  ออกจากโปรแกรม", icons::SIGN_OUT)).clicked() {
                        ui.close();
                        app.request(&ctx, PendingAction::Close);
                    }
                });

                // ชื่อโปรเจกต์ตรงกลาง (มีจุดถ้ายังไม่ได้บันทึก) และบอกว่าบันทึกล่าสุดเมื่อไร
                let name = if app.dirty { format!("{} •", app.project.name) } else { app.project.name.clone() };
                let center = ui.max_rect().center();
                let status = match (app.last_saved, app.dirty) {
                    (Some(t), _) => {
                        let min = t.elapsed().as_secs() / 60;
                        if min == 0 {
                            "บันทึกแล้วเมื่อสักครู่".to_owned()
                        } else {
                            format!("บันทึกล่าสุด {min} นาทีที่แล้ว")
                        }
                    }
                    (None, true) if app.project_path.is_none() => "ยังไม่ได้บันทึก".to_owned(),
                    _ => String::new(),
                };
                // มีบรรทัดสถานะ: ชื่อขยับขึ้นให้สองบรรทัดอยู่กลางแถบพอดี
                let name_y = if status.is_empty() { 0.0 } else { -7.0 };
                ui.painter().text(
                    center + egui::vec2(0.0, name_y),
                    egui::Align2::CENTER_CENTER,
                    name,
                    egui::FontId::proportional(14.0),
                    theme::TEXT_DIM,
                );
                if !status.is_empty() {
                    let pos = center + egui::vec2(0.0, 10.0);
                    ui.painter().text(
                        pos,
                        egui::Align2::CENTER_CENTER,
                        status,
                        egui::FontId::proportional(11.5),
                        theme::TEXT_DIM.gamma_multiply(0.8),
                    );
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs(30));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let export = egui::Button::new(RichText::new(format!("{}  ส่งออก", icons::EXPORT)).color(egui::Color32::BLACK).strong())
                        .fill(theme::ACCENT)
                        .corner_radius(6)
                        .min_size(egui::vec2(96.0, 30.0));
                    if ui.add(export).on_hover_text("ส่งออกเป็นไฟล์ MP4 (Ctrl+E)").clicked() {
                        app.open_export_dialog(&ctx);
                    }
                    if ui.button(RichText::new(icons::FLOPPY_DISK).size(16.0)).on_hover_text("บันทึกโปรเจกต์ (Ctrl+S)").clicked()
                    {
                        app.save(&ctx, false);
                    }
                });
            });
        });
}
