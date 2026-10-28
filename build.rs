//! ฝังไอคอน (assets/icon.ico) ลงไฟล์ tadtor.exe ให้ขึ้นใน File Explorer และแถบงาน

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            // ไม่มีไอคอนก็ยังใช้โปรแกรมได้ แค่เตือนไว้
            println!("cargo:warning=ฝังไอคอนลง .exe ไม่ได้: {e}");
        }
    }
}
