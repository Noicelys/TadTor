//! ตัวเลื่อนค่าแบบใช้ง่าย (ใช้แทน `egui::Slider` ทั้งโปรแกรม)
//!
//! - รางหนา ยาวเต็มพื้นที่ แถบสีไล่จากค่าเริ่มต้นไปถึงค่าปัจจุบัน (ค่าติดลบไล่ไปทางซ้าย) มีขีดบอกจุดค่าเริ่มต้น
//! - คลิกตรงไหนของรางก็กระโดดไปค่านั้น ลากได้ทั้งราง กด Shift ค้างระหว่างลาก = ปรับละเอียด 10 เท่า
//! - ดับเบิลคลิกราง หรือกดปุ่ม ↺ = คืนค่าเริ่มต้น • ช่องตัวเลขด้านขวาลากหรือพิมพ์ค่าเองได้
//!
//! คืน `Response` ที่รวมทุกส่วนไว้ ใช้กับ `TadTorApp::live_edit` ได้ตรง ๆ (ลากยาวเก็บ Undo ครั้งเดียว)

use std::ops::RangeInclusive;

use eframe::egui::{self, emath::Numeric, Sense, Stroke};

use crate::{icons, theme};

pub struct Fader<'a, N: Numeric> {
    value: &'a mut N,
    range: RangeInclusive<N>,
    /// 0 = ละเอียดต่อเนื่อง
    step: f64,
    suffix: &'a str,
    default: Option<N>,
    rail_width: Option<f32>,
    show_value: bool,
}

impl<'a, N: Numeric> Fader<'a, N> {
    pub fn new(value: &'a mut N, range: RangeInclusive<N>) -> Self {
        Self { value, range, step: 0.0, suffix: "", default: None, rail_width: None, show_value: true }
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = suffix;
        self
    }

    /// ค่าเริ่มต้น: มีปุ่ม ↺ ดับเบิลคลิกคืนค่าได้ และแถบสีไล่จากค่านี้
    pub fn default_value(mut self, d: N) -> Self {
        self.default = Some(d);
        self
    }

    /// ความยาวรางคงที่ (ปกติยาวเต็มพื้นที่ที่เหลือ)
    pub fn width(mut self, w: f32) -> Self {
        self.rail_width = Some(w);
        self
    }

    pub fn hide_value(mut self) -> Self {
        self.show_value = false;
        self
    }
}

const KNOB: f32 = 7.0;
const RAIL: f32 = 6.0;
const VALUE_W: f32 = 64.0;

impl<N: Numeric> egui::Widget for Fader<'_, N> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let (lo, hi) = (self.range.start().to_f64(), self.range.end().to_f64());
        let span = (hi - lo).max(1e-9);
        let step = if N::INTEGRAL { self.step.max(1.0) } else { self.step };
        let snap = |x: f64| {
            let x = x.clamp(lo, hi);
            if step > 0.0 {
                (lo + ((x - lo) / step).round() * step).clamp(lo, hi)
            } else {
                x
            }
        };
        let decimals = if step >= 1.0 {
            0
        } else if step >= 0.1 || step == 0.0 {
            1
        } else {
            2
        };
        let default = self.default.map(|d| d.to_f64());
        let before = self.value.to_f64();
        let mut v = before;

        let h = ui.spacing().interact_size.y;
        let gap = ui.spacing().item_spacing.x;
        let extra = if self.show_value { VALUE_W + gap } else { 0.0 } + if default.is_some() { h + gap } else { 0.0 };
        let rail_w = self.rail_width.unwrap_or_else(|| ui.available_width() - extra).max(60.0);

        // ---- ราง ----
        let (rect, mut resp) = ui.allocate_exact_size(egui::vec2(rail_w, h), Sense::click_and_drag());
        let (x0, x1) = (rect.left() + KNOB, rect.right() - KNOB);
        let to_x = |val: f64| x0 + ((val - lo) / span).clamp(0.0, 1.0) as f32 * (x1 - x0);
        let from_x = |x: f32| lo + ((x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0) as f64 * span;

        if resp.dragged() || resp.clicked() {
            let fine = ui.input(|i| i.modifiers.shift);
            if fine && resp.dragged() {
                v += resp.drag_delta().x as f64 / (x1 - x0).max(1.0) as f64 * span * 0.1;
            } else if let Some(pos) = resp.interact_pointer_pos() {
                v = from_x(pos.x);
            }
        }
        if resp.double_clicked() {
            if let Some(d) = default {
                v = d;
            }
        }
        v = snap(v);
        resp = resp.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        let tip = if default.is_some() {
            "ลากเพื่อปรับ • Shift + ลาก = ละเอียด • ดับเบิลคลิก = คืนค่าเริ่มต้น"
        } else {
            "ลากเพื่อปรับ • Shift + ลาก = ละเอียด"
        };
        if !resp.dragged() {
            resp = resp.on_hover_text_at_pointer(tip);
        }

        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            let cy = rect.center().y;
            let rail =
                egui::Rect::from_min_max(egui::pos2(rect.left() + 2.0, cy - RAIL / 2.0), egui::pos2(rect.right() - 2.0, cy + RAIL / 2.0));
            p.rect_filled(rail, RAIL / 2.0, theme::SURFACE_HOVER);
            // แถบสีจากค่าเริ่มต้น (หรือปลายซ้าย) ถึงค่าปัจจุบัน
            let origin = default.filter(|d| *d > lo && *d < hi).unwrap_or(lo);
            let (a, b) = (to_x(origin), to_x(v));
            let active = resp.dragged() || resp.hovered();
            let fill = if active { theme::ACCENT } else { theme::ACCENT.gamma_multiply(0.8) };
            if (a - b).abs() > 0.5 {
                let r = egui::Rect::from_min_max(egui::pos2(a.min(b), rail.top()), egui::pos2(a.max(b), rail.bottom()));
                p.rect_filled(r, RAIL / 2.0, fill);
            }
            if origin > lo {
                p.line_segment([egui::pos2(a, cy - 7.0), egui::pos2(a, cy + 7.0)], Stroke::new(1.5, theme::TEXT_DIM));
            }
            let knob = egui::pos2(b, cy);
            let r = if active { KNOB + 1.0 } else { KNOB };
            p.circle(knob, r, egui::Color32::WHITE, Stroke::new(if resp.dragged() { 3.0 } else { 2.0 }, fill));
        }

        // ---- ช่องตัวเลข ----
        if self.show_value {
            let mut typed = v;
            let dv = egui::DragValue::new(&mut typed).range(lo..=hi).speed(span / 300.0).max_decimals(decimals).suffix(self.suffix);
            let dv = ui.add_sized([VALUE_W, h], dv).on_hover_text("ลากซ้าย-ขวา หรือดับเบิลคลิกเพื่อพิมพ์ค่า");
            if dv.changed() {
                v = snap(typed);
            }
            resp |= dv;
        }

        // ---- ปุ่มคืนค่า ----
        if let Some(d) = default {
            let changed = (v - d).abs() > 1e-9;
            let label = egui::RichText::new(icons::ARROW_COUNTER_CLOCKWISE).color(if changed { theme::TEXT } else { theme::TEXT_DIM });
            let btn = ui
                .add_enabled_ui(changed, |ui| ui.add_sized([h, h], egui::Button::new(label).frame(false)))
                .inner
                .on_hover_text(format!("คืนค่าเริ่มต้น ({}{})", trim_num(d, decimals), self.suffix));
            if btn.clicked() {
                v = d;
            }
            resp |= btn;
        }

        if (v - before).abs() > 1e-12 {
            *self.value = N::from_f64(v);
            resp.mark_changed();
        }
        resp
    }
}

fn trim_num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    }
}
