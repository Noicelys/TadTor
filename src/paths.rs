//! ที่อยู่ของโปรแกรมและทางลัด (shortcut) บนเดสก์ท็อป/เมนู Start

use std::path::{Path, PathBuf};

/// โฟลเดอร์ของโปรแกรม (ที่หา `ffmpeg\` และ `whisper\`): โฟลเดอร์ของไฟล์ .exe
/// ยกเว้นตอนรันจาก `target\` ของ cargo ใช้โฟลเดอร์โปรเจกต์แทน
/// ไม่ขึ้นกับ current dir จึงเปิดจากทางลัดหรือให้ AI เปิดก็หาเครื่องมือเจอ
pub fn app_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "target"))
        .and_then(Path::parent)
        .or_else(|| exe.parent())
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// ไฟล์ที่ทางลัดควรชี้ไป: ตัวที่กำลังรันอยู่ ถ้าเป็นตัว debug (มีหน้าต่าง cmd) และมีตัว release แล้ว ใช้ตัว release แทน
fn shortcut_target() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let is_debug = exe.parent().is_some_and(|d| d.file_name().is_some_and(|n| n == "debug"));
    let release = exe.parent().and_then(Path::parent).map(|t| t.join("release").join("tadtor.exe"));
    Ok(match release {
        Some(r) if is_debug && r.is_file() => r,
        _ => exe,
    })
}

/// ปุ่มลัดเปิดโปรแกรมจากที่ไหนก็ได้ (ตั้งไว้ที่ทางลัดในเมนู Start)
pub const HOTKEY_LABEL: &str = "Ctrl + Alt + T";

/// สร้าง/อัปเดตทางลัด "TadTor" บนเดสก์ท็อปและในเมนู Start พร้อมปุ่มลัด Ctrl + Alt + T
#[cfg(windows)]
pub fn create_shortcuts() -> Result<(), String> {
    // ส่งที่อยู่ไฟล์ทางตัวแปรแวดล้อม (ชื่อโฟลเดอร์ภาษาไทยไม่ต้อง escape ในสคริปต์)
    // WScript.Shell ตั้งเป้าหมายเป็นที่อยู่ภาษาไทยไม่ได้ จึงสร้างไฟล์ด้วย WScript ก่อน แล้วแก้ผ่าน Shell.Application ที่รองรับ Unicode
    const SCRIPT: &str = r"
        $ErrorActionPreference = 'Stop'
        $w = New-Object -ComObject WScript.Shell
        $sh = New-Object -ComObject Shell.Application
        foreach ($d in @([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs'))) {
            $s = $w.CreateShortcut((Join-Path $d 'TadTor.lnk'))
            $s.TargetPath = $env:WINDIR + '\explorer.exe'
            $s.Save()
            $l = $sh.NameSpace($d).ParseName('TadTor.lnk').GetLink
            $l.Path = $env:TADTOR_EXE
            $l.WorkingDirectory = $env:TADTOR_DIR
            $l.Description = 'TadTor video editor'
            $l.SetIconLocation($env:TADTOR_EXE, 0)
            # ปุ่มลัดตั้งที่เดียวพอ (ซ้ำสองที่ Windows จะสับสน): Ctrl+Alt = 0x06 ไบต์บน, T = 0x54
            if ($d -eq [Environment]::GetFolderPath('Programs')) { $l.Hotkey = 0x654 }
            $l.Save()
        }";
    let target = shortcut_target()?;
    let out = crate::ffmpeg::command(Path::new("powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("TADTOR_EXE", &target)
        .env("TADTOR_DIR", app_dir())
        .output()
        .map_err(|e| format!("เรียก PowerShell ไม่ได้: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("สร้างทางลัดไม่สำเร็จ: {}", err.lines().next().unwrap_or_default().trim()));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn create_shortcuts() -> Result<(), String> {
    Err("สร้างทางลัดได้เฉพาะบน Windows".to_owned())
}
