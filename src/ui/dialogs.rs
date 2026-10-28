//! หน้าต่างถาม "บันทึกงานก่อนไหม" ก่อนปิดโปรแกรม / เปิดงานใหม่

use eframe::egui::{self, RichText};

use crate::app::TadTorApp;
use crate::theme;

pub fn show(app: &mut TadTorApp, ctx: &egui::Context) {
    let Some(action) = app.pending.clone() else { return };
    let mut choice = None;
    egui::Modal::new(egui::Id::unique("save_changes")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.label(RichText::new("บันทึกงานก่อนไหม?").strong().size(15.0));
        ui.add_space(4.0);
        ui.label(RichText::new(format!("\"{}\" มีการแก้ไขที่ยังไม่ได้บันทึก", app.project.name)).color(theme::TEXT_DIM));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let save = egui::Button::new(RichText::new("บันทึก").color(egui::Color32::BLACK).strong()).fill(theme::ACCENT);
            if ui.add(save).clicked() {
                choice = Some(Choice::Save);
            }
            if ui.button("ไม่บันทึก").clicked() {
                choice = Some(Choice::Discard);
            }
            if ui.button("ยกเลิก").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                choice = Some(Choice::Cancel);
            }
        });
    });
    match choice {
        Some(Choice::Save) => {
            app.pending = None;
            if app.save(ctx, false) {
                app.perform(ctx, action);
            }
        }
        Some(Choice::Discard) => {
            app.pending = None;
            app.perform(ctx, action);
        }
        Some(Choice::Cancel) => app.pending = None,
        None => {}
    }
}

enum Choice {
    Save,
    Discard,
    Cancel,
}
