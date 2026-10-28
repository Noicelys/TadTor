/// แปลงวินาทีเป็นข้อความเวลาแบบ ชั่วโมง:นาที:วินาที:เฟรม (เหมือนในโปรแกรมตัดต่อทั่วไป)
pub fn format_timecode(seconds: f64, fps: u32) -> String {
    let seconds = seconds.max(0.0);
    let fps = fps.max(1) as f64;
    let total_frames = (seconds * fps).floor() as u64;
    let fps_u = fps as u64;
    let frames = total_frames % fps_u;
    let total_secs = total_frames / fps_u;
    let s = total_secs % 60;
    let m = (total_secs / 60) % 60;
    let h = total_secs / 3600;
    format!("{h:02}:{m:02}:{s:02}:{frames:02}")
}

/// แบบสั้น นาที:วินาที ใช้บนไม้บรรทัดของไทม์ไลน์
pub fn format_short(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecode_basic() {
        assert_eq!(format_timecode(0.0, 30), "00:00:00:00");
        assert_eq!(format_timecode(1.5, 30), "00:00:01:15");
        assert_eq!(format_timecode(3661.0, 30), "01:01:01:00");
        assert_eq!(format_short(75.0), "01:15");
    }
}
