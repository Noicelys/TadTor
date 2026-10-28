//! อ่าน/เขียนไฟล์ซับไตเติล .srt

/// แปลงข้อความ .srt เป็นรายการ (เริ่ม, จบ, ข้อความ)
pub fn parse_srt(text: &str) -> Vec<(f64, f64, String)> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let mut lines = block.lines().map(str::trim_end).skip_while(|l| l.trim().is_empty());
        let mut first = lines.next();
        // บรรทัดแรกเป็นเลขลำดับ (บางไฟล์ไม่มี)
        if first.is_some_and(|l| !l.contains("-->")) {
            first = lines.next();
        }
        let Some((a, b)) = first.and_then(|l| l.split_once("-->")) else { continue };
        let (Some(s), Some(e)) = (parse_time(a), parse_time(b)) else { continue };
        let body: Vec<&str> = lines.collect();
        let body = strip_tags(&body.join("\n"));
        if e > s && !body.trim().is_empty() {
            out.push((s, e, body.trim().to_owned()));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// "00:01:02,345" → 62.345
fn parse_time(s: &str) -> Option<f64> {
    let s = s.split_whitespace().next()?.replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (h.parse::<f64>().ok()?, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        [m, s] => (0.0, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// ลบแท็ก <i> <b> <font …> ที่บางไฟล์ใส่มา
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn format_time(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

pub fn write_srt(items: &[(f64, f64, String)]) -> String {
    let mut out = String::new();
    for (i, (s, e, text)) in items.iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, format_time(*s), format_time(*e), text.trim()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let src = "\u{feff}1\r\n00:00:01,000 --> 00:00:02,500\r\nสวัสดีครับ\r\n<i>บรรทัดสอง</i>\r\n\r\n2\r\n00:01:00,250 --> 00:01:03,000\r\nลาก่อน\r\n";
        let items = parse_srt(src);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], (1.0, 2.5, "สวัสดีครับ\nบรรทัดสอง".to_owned()));
        assert!((items[1].0 - 60.25).abs() < 1e-9);
        let back = parse_srt(&write_srt(&items));
        assert_eq!(back, items);
        assert_eq!(format_time(3725.5), "01:02:05,500");
    }
}
