//! วิเคราะห์เสียงพูด (ไม่ยุ่งกับหน้าจอ ทดสอบได้)
//!
//! - หาช่วงที่มีเสียงจาก PCM โมโน: ใช้แบ่งเสียงก่อนส่งให้ whisper (เวลาซับจึงตรง และไม่ถอดช่วงเงียบจนได้คำมั่ว)
//!   และใช้หาช่วงเงียบให้ตัดทิ้ง
//! - แบ่งซับยาวเป็นหลายบรรทัด รองรับภาษาไทยที่ไม่เว้นวรรค (ไม่ตัดกลางสระ/วรรณยุกต์)

/// อ่านไฟล์ WAV แบบ PCM 16 บิต คืน (อัตราสุ่ม, ตัวอย่างเสียงของช่องแรก)
pub fn read_wav_s16(bytes: &[u8]) -> Option<(u32, Vec<i16>)> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let (mut rate, mut channels, mut bits) = (0u32, 1usize, 0u16);
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32_at(i + 4) as usize;
        let body = i + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = u16_at(body + 2).max(1) as usize;
            rate = u32_at(body + 4);
            bits = u16_at(body + 14);
        } else if id == b"data" {
            if bits != 16 || rate == 0 {
                return None;
            }
            // WAV ที่ส่งทาง pipe ไม่รู้ความยาวล่วงหน้า (ขนาดเป็น 0 หรือค่าสูงสุด) = อ่านถึงท้ายไฟล์
            let end = if len == 0 || len == u32::MAX as usize { bytes.len() } else { (body + len).min(bytes.len()) };
            let samples = bytes[body..end].chunks_exact(2 * channels).map(|f| i16::from_le_bytes([f[0], f[1]])).collect();
            return Some((rate, samples));
        }
        i = body + len + (len & 1);
    }
    None
}

/// เขียน WAV PCM 16 บิต โมโน
pub fn wav_s16(rate: u32, samples: &[i16]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut v = Vec::with_capacity(44 + data as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // โมโน
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * 2).to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

#[derive(Clone, Copy, Debug)]
pub struct VadSettings {
    /// ช่วงเงียบสั้นกว่านี้ (วินาที) ถือว่ายังพูดต่อเนื่อง
    pub min_gap: f64,
    /// เติมหัวท้ายช่วงเสียง (วินาที) กันคำแรก/คำสุดท้ายขาด
    pub pad: f64,
    /// ดังกว่าระดับเสียงพื้นหลังเท่านี้ (dB) จึงนับว่ามีเสียง
    pub above_floor_db: f32,
}

impl Default for VadSettings {
    fn default() -> Self {
        Self { min_gap: 0.3, pad: 0.15, above_floor_db: 12.0 }
    }
}

const FRAME: f64 = 0.02;

/// ช่วง [เริ่ม, จบ) (วินาที) ที่มีเสียงดังกว่าเสียงพื้นหลังชัดเจน
pub fn speech_regions(samples: &[i16], rate: u32, s: VadSettings) -> Vec<(f64, f64)> {
    let n = ((rate as f64 * FRAME) as usize).max(1);
    let db: Vec<f32> = samples
        .chunks(n)
        .map(|f| {
            let sum: f64 = f.iter().map(|&x| (x as f64) * (x as f64)).sum();
            let rms = (sum / f.len() as f64).sqrt() / 32768.0;
            (20.0 * rms.max(1e-6).log10()) as f32
        })
        .collect();
    if db.is_empty() {
        return Vec::new();
    }
    // ระดับเสียงพื้นหลัง = เฟรมที่เบาที่ 10%
    let mut sorted = db.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    let threshold = (floor + s.above_floor_db).clamp(-55.0, -28.0);

    let mut regions: Vec<(f64, f64)> = Vec::new();
    let mut start = None;
    for (i, &d) in db.iter().chain(std::iter::once(&f32::MIN)).enumerate() {
        let t = i as f64 * FRAME;
        match (d > threshold, start) {
            (true, None) => start = Some(t),
            (false, Some(s0)) => {
                regions.push((s0, t));
                start = None;
            }
            _ => {}
        }
    }
    let total = samples.len() as f64 / rate as f64;
    // รวมช่วงที่เว้นสั้น ๆ ทิ้งเสียงแวบสั้นมาก แล้วเติมหัวท้าย
    let merged = merge(regions, s.min_gap);
    let kept = merged.into_iter().filter(|(a, b)| b - a >= 0.15).map(|(a, b)| ((a - s.pad).max(0.0), (b + s.pad).min(total)));
    merge(kept.collect(), 0.0)
}

fn merge(regions: Vec<(f64, f64)>, gap: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (a, b) in regions {
        match out.last_mut() {
            Some(last) if a - last.1 <= gap + 1e-9 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// รวมช่วงเสียงเป็นก้อนสำหรับส่งให้ whisper: ก้อนละไม่เกิน `max_len` วินาที และแยกก้อนเมื่อเงียบนานกว่า `split_gap`
/// (whisper ประมวลผลทีละ 30 วินาทีเสมอ ก้อนเล็กเกินไปจึงช้า ก้อนที่มีช่วงเงียบยาวข้างในเวลาจะคลาด)
pub fn group_chunks(regions: &[(f64, f64)], split_gap: f64, max_len: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    for &(a, b) in regions {
        match out.last_mut() {
            Some(last) if a - last.1 < split_gap && b - last.0 <= max_len => last.1 = b,
            _ => out.push((a, b)),
        }
    }
    out
}

/// ช่วงเงียบ (ระหว่างช่วงเสียง) ที่ยาวอย่างน้อย `min_len` วินาที ในเสียงยาว `total`
pub fn silences(regions: &[(f64, f64)], total: f64, min_len: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut prev = 0.0;
    for &(a, b) in regions.iter().chain(std::iter::once(&(total, total))) {
        if a - prev >= min_len {
            out.push((prev, a));
        }
        prev = prev.max(b);
    }
    out
}

/// เลื่อนเวลา `t` ในช่วง [start, end] ให้อยู่บนช่วงที่มีเสียงจริง
/// (แบ่งเวลาตามสัดส่วนแล้วข้ามช่วงเงียบ บรรทัดถัดไปจึงไม่ขึ้นก่อนคนพูด)
pub fn spread_over(regions: &[(f64, f64)], start: f64, end: f64, t: f64) -> f64 {
    let parts: Vec<(f64, f64)> = regions.iter().map(|&(a, b)| (a.max(start), b.min(end))).filter(|(a, b)| b > a).collect();
    let speech: f64 = parts.iter().map(|(a, b)| b - a).sum();
    if speech < 1e-6 || end - start < 1e-6 {
        return t;
    }
    let mut want = (t - start) / (end - start) * speech;
    for &(a, b) in &parts {
        if want < b - a - 1e-9 {
            return a + want;
        }
        want -= b - a;
    }
    parts.last().map_or(t, |p| p.1)
}

/// แก้ข้อความจาก whisper: สระอำที่แยกเป็นสองตัว (ํ + า) ตัวอักษรล่องหน ช่องว่างซ้ำ
pub fn clean_text(t: &str) -> String {
    let t = t.replace("\u{0E4D}\u{0E32}", "\u{0E33}").replace(['\u{200B}', '\u{FEFF}', '\u{FFFD}'], "");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// ตัดบรรทัดที่ซ้ำติดกัน (whisper ชอบวนพูดประโยคเดิมซ้ำ) โดยยืดบรรทัดแรกแทน
pub fn drop_repeats(items: Vec<(f64, f64, String)>) -> Vec<(f64, f64, String)> {
    let mut out: Vec<(f64, f64, String)> = Vec::new();
    for it in items {
        match out.last_mut() {
            Some(last) if last.2 == it.2 && it.0 - last.1 < 1.0 => last.1 = last.1.max(it.1),
            _ => out.push(it),
        }
    }
    out
}

// ---------------- แบ่งบรรทัด ----------------

/// สระ/วรรณยุกต์ไทยที่อยู่บน-ล่างตัวอักษร (ไม่นับความยาว และห้ามขึ้นต้นบรรทัด)
fn is_thai_mark(c: char) -> bool {
    matches!(c, '\u{0E31}' | '\u{0E34}'..='\u{0E3A}' | '\u{0E47}'..='\u{0E4E}')
}

fn is_thai(c: char) -> bool {
    ('\u{0E00}'..='\u{0E7F}').contains(&c)
}

/// ความยาวที่ตาเห็น (ไม่นับสระบน-ล่างและวรรณยุกต์)
pub fn visible_len(s: &str) -> usize {
    s.chars().filter(|&c| !is_thai_mark(c)).count()
}

/// รอยต่อคำจากตัวตัดคำ ICU ที่มากับ Windows 10 ขึ้นไป (รู้จักคำไทยจากพจนานุกรม)
/// คืน `v[i]` = ตัดก่อนตัวที่ i ได้ (None = เครื่องนี้ไม่มี ICU)
#[cfg(windows)]
fn word_boundaries(chars: &[char]) -> Option<Vec<bool>> {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    type Open = unsafe extern "C" fn(i32, *const u8, *const u16, i32, *mut i32) -> *mut c_void;
    type Step = unsafe extern "C" fn(*mut c_void) -> i32;
    type Close = unsafe extern "C" fn(*mut c_void);
    struct Icu {
        open: Open,
        next: Step,
        close: Close,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    }
    static ICU: OnceLock<Option<Icu>> = OnceLock::new();
    let icu = ICU
        .get_or_init(|| unsafe {
            let name: Vec<u16> = "icu.dll\0".encode_utf16().collect();
            let h = LoadLibraryW(name.as_ptr());
            if h.is_null() {
                return None;
            }
            let f = |n: &[u8]| Some(GetProcAddress(h, n.as_ptr())).filter(|p| !p.is_null());
            Some(Icu {
                open: std::mem::transmute::<*mut c_void, Open>(f(b"ubrk_open\0")?),
                next: std::mem::transmute::<*mut c_void, Step>(f(b"ubrk_next\0")?),
                close: std::mem::transmute::<*mut c_void, Close>(f(b"ubrk_close\0")?),
            })
        })
        .as_ref()?;

    let text: String = chars.iter().collect();
    let utf16: Vec<u16> = text.encode_utf16().collect();
    // ตำแหน่ง UTF-16 → ลำดับตัวอักษร
    let mut char_at = vec![usize::MAX; utf16.len() + 1];
    let mut pos = 0;
    for (i, c) in chars.iter().enumerate() {
        char_at[pos] = i;
        pos += c.len_utf16();
    }
    char_at[pos] = chars.len();
    let mut out = vec![false; chars.len() + 1];
    unsafe {
        let mut status = 0i32;
        // 1 = UBRK_WORD
        let bi = (icu.open)(1, c"th".as_ptr().cast(), utf16.as_ptr(), utf16.len() as i32, &mut status);
        if bi.is_null() || status > 0 {
            return None;
        }
        loop {
            let b = (icu.next)(bi);
            if b < 0 {
                break;
            }
            if let Some(&i) = char_at.get(b as usize).filter(|&&i| i != usize::MAX) {
                out[i] = true;
            }
        }
        (icu.close)(bi);
    }
    Some(out)
}

#[cfg(not(windows))]
fn word_boundaries(_chars: &[char]) -> Option<Vec<bool>> {
    None
}

/// คะแนนของจุดตัดระหว่าง `a` กับ `b` (0 = ห้ามตัด) เมื่อไม่มีตัวตัดคำ
fn break_score(a: char, b: char) -> u8 {
    if is_thai_mark(b) || matches!(a, 'เ' | 'แ' | 'โ' | 'ใ' | 'ไ') || matches!(b, 'ะ' | 'า' | 'ำ' | 'ๆ' | 'ฯ' | 'ๅ') {
        return 0;
    }
    if a.is_whitespace() || b.is_whitespace() {
        return 4;
    }
    if is_thai(a) != is_thai(b) {
        return 3;
    }
    // ไทย: ก่อนสระหน้า หรือหลังสระที่มักจบพยางค์ มักเป็นรอยต่อคำ
    if matches!(b, 'เ' | 'แ' | 'โ' | 'ใ' | 'ไ') || matches!(a, 'ะ' | 'า' | 'ำ' | 'ๆ') {
        return 2;
    }
    if a.is_alphanumeric() && b.is_alphanumeric() && !is_thai(a) {
        return 0; // ไม่ตัดกลางคำภาษาอังกฤษ/ตัวเลข
    }
    1
}

/// แบ่งข้อความยาวเป็นหลายบรรทัด บรรทัดละไม่เกิน `max_len` ตัวอักษร (0 = ไม่แบ่ง)
/// เวลาแบ่งตามสัดส่วนความยาวข้อความ
pub fn split_line(text: &str, start: f64, end: f64, max_len: usize) -> Vec<(f64, f64, String)> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let total = visible_len(&text);
    if text.is_empty() {
        return Vec::new();
    }
    if max_len == 0 || total <= max_len {
        return vec![(start, end, text)];
    }
    let chars: Vec<char> = text.chars().collect();
    // นับความยาวสะสมถึงก่อนตัวที่ i
    let mut upto = vec![0usize; chars.len() + 1];
    for (i, &c) in chars.iter().enumerate() {
        upto[i + 1] = upto[i] + usize::from(!is_thai_mark(c));
    }
    let lines = total.div_ceil(max_len);
    let target = total as f64 / lines as f64;
    let words = word_boundaries(&chars);
    let score_at = |i: usize| -> u8 {
        let (a, b) = (chars[i - 1], chars[i]);
        match &words {
            // มีพจนานุกรม: ตัดได้เฉพาะรอยต่อคำ (ช่องว่างดีที่สุด)
            Some(w) if is_thai(a) || is_thai(b) => {
                if a.is_whitespace() || b.is_whitespace() {
                    4
                } else if w[i] && !is_thai_mark(b) {
                    3
                } else {
                    0
                }
            }
            _ => break_score(a, b),
        }
    };

    let mut pieces: Vec<String> = Vec::new();
    let mut from = 0usize;
    while upto[chars.len()] - upto[from] > max_len {
        let want = upto[from] as f64 + target;
        let mut best: Option<(f64, usize)> = None;
        for i in from + 1..chars.len() {
            let len = upto[i] - upto[from];
            if len > max_len {
                break;
            }
            let score = score_at(i);
            if score == 0 || (len as f64) < target * 0.5 {
                continue;
            }
            // ใกล้ความยาวเป้าหมาย + จุดตัดดี = ดี
            let cost = (upto[i] as f64 - want).abs() - score as f64 * 4.0;
            if best.is_none_or(|b| cost < b.0) {
                best = Some((cost, i));
            }
        }
        let cut = best.map(|b| b.1).unwrap_or_else(|| {
            // ไม่มีจุดตัดที่ดี: ตัดตรงไหนก็ได้ที่ไม่ผ่าสระ/วรรณยุกต์
            (from + 1..chars.len()).rev().find(|&i| upto[i] - upto[from] <= max_len && !is_thai_mark(chars[i])).unwrap_or(from + 1)
        });
        pieces.push(chars[from..cut].iter().collect::<String>().trim().to_owned());
        from = cut;
    }
    pieces.push(chars[from..].iter().collect::<String>().trim().to_owned());

    let span = (end - start).max(0.0);
    let mut t = start;
    let mut out = Vec::new();
    for p in pieces.into_iter().filter(|p| !p.is_empty()) {
        let d = span * visible_len(&p) as f64 / total.max(1) as f64;
        out.push((t, (t + d).min(end), p));
        t += d;
    }
    if let Some(last) = out.last_mut() {
        last.1 = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, secs: f64, amp: f64) -> Vec<i16> {
        (0..(rate as f64 * secs) as usize).map(|i| ((i as f64 * 0.07).sin() * amp * 32767.0) as i16).collect()
    }

    #[test]
    fn finds_speech_between_silences() {
        let rate = 16_000;
        let mut s = tone(rate, 1.0, 0.0005);
        s.extend(tone(rate, 2.0, 0.3));
        s.extend(tone(rate, 3.0, 0.0005));
        s.extend(tone(rate, 1.0, 0.3));
        s.extend(tone(rate, 0.5, 0.0005));
        let r = speech_regions(&s, rate, VadSettings::default());
        assert_eq!(r.len(), 2, "{r:?}");
        assert!((r[0].0 - 0.85).abs() < 0.05 && (r[0].1 - 3.15).abs() < 0.05, "{r:?}");
        assert!((r[1].0 - 5.85).abs() < 0.05, "{r:?}");
        let sil = silences(&r, 7.5, 1.0);
        assert_eq!(sil.len(), 1);
        assert!((sil[0].0 - 3.15).abs() < 0.05 && (sil[0].1 - 5.85).abs() < 0.05);
        // เงียบ 2.7 วิ มากกว่า split_gap = แยกก้อน
        assert_eq!(group_chunks(&r, 1.0, 25.0).len(), 2);
        assert_eq!(group_chunks(&r, 5.0, 25.0), [(r[0].0, r[1].1)]);
    }

    #[test]
    fn wav_roundtrip() {
        let s = vec![0i16, 1000, -1000, 32767];
        let (rate, back) = read_wav_s16(&wav_s16(16_000, &s)).unwrap();
        assert_eq!((rate, back), (16_000, s));
    }

    #[test]
    fn thai_lines_split_cleanly() {
        let text = "สวัสดีครับวันนี้เราจะมาสอนวิธีตัดต่อวิดีโอด้วยโปรแกรมของเราเองขั้นแรกให้นำเข้าไฟล์วิดีโอที่ต้องการ";
        let lines = split_line(text, 10.0, 20.0, 30);
        assert!(lines.len() >= 3, "{lines:?}");
        let joined: String = lines.iter().map(|l| l.2.as_str()).collect();
        assert_eq!(joined, text, "ต้องไม่หายหรือเพิ่มตัวอักษร");
        for (s, e, l) in &lines {
            assert!(visible_len(l) <= 30, "{l}");
            assert!(!l.starts_with(is_thai_mark), "บรรทัดห้ามขึ้นต้นด้วยสระบน/ล่าง: {l}");
            assert!(e > s);
        }
        assert_eq!(lines[0].0, 10.0);
        assert_eq!(lines.last().unwrap().1, 20.0);
    }

    #[test]
    fn spread_skips_silence() {
        // พูด 0..2 และ 4..6 ในซับ 0..6: กลางซับ (3 วิ) ต้องไปอยู่ต้นช่วงพูดที่สอง ไม่ใช่กลางช่วงเงียบ
        let r = [(0.0, 2.0), (4.0, 6.0)];
        assert!((spread_over(&r, 0.0, 6.0, 3.0) - 4.0).abs() < 1e-9);
        assert!((spread_over(&r, 0.0, 6.0, 1.5) - 3.0).abs() > 0.5);
        assert_eq!(spread_over(&[], 0.0, 6.0, 3.0), 3.0);
    }

    #[test]
    fn cleans_whisper_text() {
        assert_eq!(clean_text("  น\u{0E4D}\u{0E32}เข้า   ไฟล์ "), "นำเข้า ไฟล์");
        let r = drop_repeats(vec![(0.0, 1.0, "ก".into()), (1.2, 2.0, "ก".into()), (2.0, 3.0, "ข".into())]);
        assert_eq!(r, [(0.0, 2.0, "ก".to_owned()), (2.0, 3.0, "ข".to_owned())]);
    }

    #[cfg(windows)]
    #[test]
    fn icu_keeps_thai_words_whole() {
        let lines = split_line("สวัสดีครับวันนี้เราจะมาสอนวิธีตัดต่อวิดีโอด้วยโปรแกรมของเรา", 0.0, 5.0, 20);
        for l in &lines {
            assert!(!l.2.starts_with("โอ") && !l.2.ends_with("วิดี") && !l.2.ends_with("โปร"), "{lines:?}");
        }
    }

    #[test]
    fn english_splits_at_spaces() {
        let lines = split_line("the quick brown fox jumps over the lazy dog again", 0.0, 5.0, 20);
        assert!(lines.iter().all(|l| !l.2.starts_with(' ') && visible_len(&l.2) <= 20), "{lines:?}");
        assert!(lines.iter().all(|l| l.2.split(' ').all(|w| "the quick brown fox jumps over lazy dog again".contains(w))));
        assert_eq!(split_line("สั้น", 0.0, 1.0, 20).len(), 1);
        assert_eq!(split_line("ยาวมากแต่ไม่จำกัด", 0.0, 1.0, 0).len(), 1);
    }
}
