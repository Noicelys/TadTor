use std::path::PathBuf;

use eframe::egui::{self, Align2, FontId, RichText, Sense};

use crate::app::{LeftTab, MediaDrag, TadTorApp};
use crate::core::media::MediaState;
use crate::core::time::format_short;
use crate::{icons, theme};

/// การ์ดสื่อและเครื่องมือ (วางในช่องไหนก็ได้ ดู `layout`)
pub fn content(app: &mut TadTorApp, ui: &mut egui::Ui) {
    tab_strip(app, ui);
    ui.add_space(2.0);
    match app.left_tab {
        LeftTab::Media => media_tab(app, ui),
        other => super::tools_panel::show(app, ui, other),
    }
}

/// แถบไอคอน + ชื่อ ด้านบนของแผงซ้าย
fn tab_strip(app: &mut TadTorApp, ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(theme::PANEL_HEADER)
        .corner_radius(egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
        .inner_margin(egui::Margin::symmetric(4, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let count = LeftTab::ALL.len() as f32;
            let w = ((ui.available_width() - 18.0) / count).floor();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                super::layout::grip(app, ui, super::layout::Card::Media);
                ui.add_space(2.0);
                for tab in LeftTab::ALL {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 46.0), Sense::click());
                    let selected = app.left_tab == tab;
                    let color = if selected {
                        theme::ACCENT
                    } else if resp.hovered() {
                        theme::TEXT
                    } else {
                        theme::TEXT_DIM
                    };
                    let p = ui.painter();
                    if resp.hovered() && !selected {
                        p.rect_filled(rect.shrink(2.0), 6, theme::SURFACE);
                    }
                    p.text(rect.center_top() + egui::vec2(0.0, 14.0), Align2::CENTER_CENTER, tab.icon(), FontId::proportional(17.0), color);
                    p.text(
                        rect.center_bottom() - egui::vec2(0.0, 11.0),
                        Align2::CENTER_CENTER,
                        tab.label(),
                        FontId::proportional(12.5),
                        color,
                    );
                    if resp.clicked() {
                        app.left_tab = tab;
                    }
                }
            });
        });
}

fn media_tab(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let hovering_files = ctx.input(|i| !i.raw.hovered_files.is_empty());

    egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("ในเครื่อง").strong());
            ui.label(RichText::new(format!("({})", app.library.items.len())).color(theme::TEXT_DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("{} นำเข้า", icons::PLUS)).clicked() {
                    app.import_dialog(&ctx);
                }
            });
        });
        ui.add_space(6.0);

        if app.library.items.is_empty() {
            drop_zone(app, ui, hovering_files);
        } else {
            media_grid(app, ui);
            if hovering_files {
                let r = ui.max_rect();
                ui.painter().rect_filled(r, 8, theme::ACCENT_DARK.gamma_multiply(0.35));
                super::dashed_rect(ui.painter(), r.shrink(2.0), theme::ACCENT);
            }
        }
    });
}

/// พื้นที่ว่างใหญ่ ๆ ให้กดนำเข้าหรือลากไฟล์มาวาง
fn drop_zone(app: &mut TadTorApp, ui: &mut egui::Ui, hovering: bool) {
    let size = egui::vec2(ui.available_width(), (ui.available_height() - 8.0).clamp(160.0, 320.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let bg = if hovering {
        theme::ACCENT_DARK.gamma_multiply(0.45)
    } else if resp.hovered() {
        theme::SURFACE
    } else {
        theme::PANEL_HEADER
    };
    p.rect_filled(rect, 8, bg);
    super::dashed_rect(p, rect.shrink(1.0), if hovering { theme::ACCENT } else { theme::BORDER });

    let c = rect.center();
    p.text(c - egui::vec2(0.0, 26.0), Align2::CENTER_CENTER, icons::TRAY_ARROW_DOWN, FontId::proportional(32.0), theme::ACCENT);
    p.text(c + egui::vec2(0.0, 10.0), Align2::CENTER_CENTER, "นำเข้า", FontId::proportional(15.0), theme::TEXT);
    p.text(
        c + egui::vec2(0.0, 34.0),
        Align2::CENTER_CENTER,
        "คลิก หรือลากวิดีโอ เสียง รูปภาพ มาวางที่นี่",
        FontId::proportional(13.0),
        theme::TEXT_DIM,
    );
    if resp.clicked() {
        let ctx = ui.ctx().clone();
        app.import_dialog(&ctx);
    }
}

/// การ์ดไฟล์สื่อแบบตาราง: ภาพย่อ ความยาว ปุ่ม + และลากลงไทม์ไลน์ได้
fn media_grid(app: &mut TadTorApp, ui: &mut egui::Ui) {
    let card_w = 112.0;
    let thumb_h = 64.0;
    let gap = 8.0;
    let cols = ((ui.available_width() + gap) / (card_w + gap)).floor().max(1.0) as usize;
    let ctx = ui.ctx().clone();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let mut clicked = None;
        let mut add = None;
        let paths: Vec<PathBuf> = app.library.items.iter().map(|it| it.path.clone()).collect();
        for row in paths.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for path in row {
                    let Some(item) = app.library.get(path) else { continue };
                    let drag_id = egui::Id::unique(("media_drag", path));
                    let payload = MediaDrag { path: path.clone() };
                    let inner = ui.dnd_drag_source(drag_id, payload, |ui| {
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(card_w, thumb_h + 22.0), Sense::click());
                        let thumb = egui::Rect::from_min_size(rect.min, egui::vec2(card_w, thumb_h));
                        let p = ui.painter();
                        p.rect_filled(thumb, 6, theme::SURFACE);
                        match app.thumbs.get(path) {
                            Some(tex) => {
                                let [w, h] = tex.size();
                                let fit = super::fit_rect(thumb, w as f32 / h.max(1) as f32);
                                p.image(
                                    tex.id(),
                                    fit,
                                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                    egui::Color32::WHITE,
                                );
                            }
                            None => {
                                p.text(
                                    thumb.center(),
                                    Align2::CENTER_CENTER,
                                    item.kind.icon(),
                                    FontId::proportional(22.0),
                                    theme::TEXT_DIM,
                                );
                            }
                        }
                        // มุมขวาล่าง: ความยาว / สถานะ
                        let badge = match &item.state {
                            MediaState::Probing => Some(("…".to_owned(), theme::TEXT_DIM)),
                            MediaState::Failed(_) => Some(("ใช้ไม่ได้".to_owned(), theme::ERROR)),
                            MediaState::Ready(info) => info.duration.map(|d| (format_short(d.ceil()), theme::TEXT)),
                        };
                        if let Some((text, color)) = badge {
                            let pos = thumb.right_bottom() - egui::vec2(4.0, 4.0);
                            let galley = p.layout_no_wrap(text, FontId::proportional(11.5), color);
                            let bg =
                                egui::Rect::from_min_size(pos - galley.size() - egui::vec2(6.0, 2.0), galley.size() + egui::vec2(6.0, 2.0));
                            p.rect_filled(bg, 3, egui::Color32::from_black_alpha(170));
                            p.galley(bg.min + egui::vec2(3.0, 1.0), galley, color);
                        }
                        let selected = app.selected_media.as_deref() == Some(path.as_path());
                        if selected || resp.hovered() {
                            let stroke_color = if selected { theme::ACCENT } else { theme::TEXT_DIM };
                            p.rect_stroke(thumb, 6, egui::Stroke::new(1.5, stroke_color), egui::StrokeKind::Inside);
                        }
                        p.text(
                            egui::pos2(rect.left() + 2.0, thumb.bottom() + 11.0),
                            Align2::LEFT_CENTER,
                            super::ellipsize(&item.name, 15),
                            FontId::proportional(12.5),
                            theme::TEXT,
                        );
                        // ปุ่ม + มุมขวาบน (ตอนชี้เมาส์) = ใส่ลงท้ายไทม์ไลน์
                        let plus_r = egui::Rect::from_min_size(thumb.right_top() + egui::vec2(-26.0, 4.0), egui::vec2(22.0, 22.0));
                        let show_plus = resp.hovered() || ui.rect_contains_pointer(plus_r);
                        let plus = ui.interact(plus_r, egui::Id::unique(("media_add", path)), Sense::click());
                        if show_plus && matches!(item.state, MediaState::Ready(_)) {
                            let fill = if plus.hovered() { theme::ACCENT } else { theme::ACCENT_DARK };
                            ui.painter().rect_filled(plus_r, 11, fill);
                            ui.painter().text(
                                plus_r.center(),
                                Align2::CENTER_CENTER,
                                icons::PLUS,
                                FontId::proportional(13.0),
                                egui::Color32::WHITE,
                            );
                        }
                        if plus.clicked() {
                            add = Some(path.clone());
                        } else if resp.clicked() {
                            clicked = Some(path.clone());
                        }
                        if resp.double_clicked() {
                            add = Some(path.clone());
                        }
                    });
                    let tip = match &item.state {
                        MediaState::Failed(e) => format!("{}\n{} {e}", item.path.display(), icons::WARNING),
                        _ => format!("{}\nลากลงไทม์ไลน์ หรือดับเบิลคลิกเพื่อเพิ่มต่อท้าย", item.path.display()),
                    };
                    inner.response.on_hover_text(tip);
                }
            });
            ui.add_space(gap);
        }
        if let Some(path) = clicked {
            app.selected_clip = None;
            app.selected_media = if app.selected_media.as_ref() == Some(&path) { None } else { Some(path) };
        }
        if let Some(path) = add {
            app.add_to_timeline(&ctx, &path, None, None);
        }
    });
}
