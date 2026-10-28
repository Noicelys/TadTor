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
                    ui.menu_button(format!("{}  จัดหน้าจอ", icons::SLIDERS_HORIZONTAL), |ui| {
                        super::layout::menu(app, ui)
                    });
                    ui.separator();
                    if ui.button(format!("{}  ออกจากโปรแกรม", icons::SIGN_OUT)).clicked() {
                        ui.close();
                        app.request(&ctx, PendingAction::Close);
                    }
                });

                // ชื่อโปรเจกต์ตรงกลาง (มีจุดถ้ายังไม่ได้บันทึก)
                let name = if app.dirty { format!("{} •", app.project.name) } else { app.project.name.clone() };
                let center = ui.max_rect().center();
                ui.painter().text(center, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(14.0), theme::TEXT_DIM);

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
