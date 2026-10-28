// ซ่อนหน้าต่าง console สีดำตอนรันแบบ release (ตอน debug ยังเห็น log ได้)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod core;
mod engine;
mod ffmpeg;
mod fonts;
mod icons;
mod mcp;
mod theme;
mod ui;

/// ไอคอนโปรแกรม (ย่อจาก assets/icon-source.png ไว้ที่ assets/icon-256.png)
pub fn app_icon() -> Option<eframe::egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")).ok()
}

fn main() -> eframe::Result {
    // tadtor.exe --mcp = เซิร์ฟเวอร์ MCP ให้ AI สั่งงานโปรแกรม (ไม่เปิดหน้าต่าง)
    if std::env::args().any(|a| a == "--mcp") {
        mcp::bridge::run();
        return Ok(());
    }
    fonts::warm_up();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("TadTor — โปรแกรมตัดต่อวิดีโอ")
            .with_inner_size([1440.0, 880.0])
            .with_min_inner_size([1024.0, 640.0])
            // เปิดเต็มจอ: จอโน้ตบุ๊กหลายเครื่องเล็กกว่าขนาดหน้าต่างด้านบน
            .with_maximized(true)
            .with_drag_and_drop(true)
            .with_icon(app_icon().unwrap_or_default()),
        // ใช้การ์ดจอผ่าน wgpu (DirectX 12 / Vulkan บน Windows)
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native("TadTor", options, Box::new(|cc| Ok(Box::new(app::TadTorApp::new(cc)))))
}
