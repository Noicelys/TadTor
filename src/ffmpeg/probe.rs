//! อ่านข้อมูลไฟล์ (ffprobe), ทำภาพย่อ, และอ่านรูปคลื่นเสียง

use std::io::Read;
use std::path::Path;
use std::process::Stdio;

use serde_json::Value;

use super::command;
use crate::core::media::{MediaInfo, MediaKind};

/// อัตรารูปคลื่นเสียง: กี่จุดต่อ 1 วินาที
pub const WAVE_PER_SEC: usize = 100;
const WAVE_RATE: usize = 8000;

pub fn probe(ffprobe: &Path, path: &Path, kind: MediaKind) -> Result<MediaInfo, String> {
    let out = command(ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| format!("เรียก ffprobe ไม่ได้: {e}"))?;
    if !out.status.success() {
        return Err(last_line(&out.stderr).unwrap_or_else(|| "อ่านไฟล์ไม่ได้".to_owned()));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();

    let video = streams.iter().find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"].as_i64() != Some(1));
    let has_audio = kind != MediaKind::Image && streams.iter().any(|s| s["codec_type"] == "audio");
    let has_video = kind != MediaKind::Audio && video.is_some();

    let (mut width, mut height, mut fps) = (0, 0, None);
    if let (true, Some(vs)) = (has_video, video) {
        width = vs["width"].as_u64().unwrap_or(0) as u32;
        height = vs["height"].as_u64().unwrap_or(0) as u32;
        // วิดีโอแนวตั้งจากมือถือเก็บภาพแนวนอนไว้แล้วบอกให้หมุน 90°
        if rotation(vs).is_some_and(|r| (r.abs() / 90) % 2 == 1) {
            std::mem::swap(&mut width, &mut height);
        }
        fps = parse_rate(vs["avg_frame_rate"].as_str()).or_else(|| parse_rate(vs["r_frame_rate"].as_str()));
    }

    let duration = match kind {
        MediaKind::Image => None,
        _ => {
            let from_stream = || {
                streams
                    .iter()
                    .filter_map(|s| s["duration"].as_str()?.parse::<f64>().ok())
                    .fold(None, |a: Option<f64>, d| Some(a.map_or(d, |a| a.max(d))))
            };
            let d = v["format"]["duration"].as_str().and_then(|s| s.parse::<f64>().ok()).or_else(from_stream);
            match d {
                Some(d) if d > 0.0 => Some(d),
                _ => return Err("ไม่ทราบความยาวไฟล์".to_owned()),
            }
        }
    };

    match kind {
        MediaKind::Video | MediaKind::Image if !has_video => return Err("ไฟล์นี้ไม่มีภาพ".to_owned()),
        MediaKind::Audio if !has_audio => return Err("ไฟล์นี้ไม่มีเสียง".to_owned()),
        _ => {}
    }

    Ok(MediaInfo { duration, width, height, fps, has_video, has_audio })
}

fn rotation(stream: &Value) -> Option<i64> {
    if let Some(r) = stream["tags"]["rotate"].as_str().and_then(|s| s.parse::<i64>().ok()) {
        return Some(r);
    }
    stream["side_data_list"].as_array()?.iter().find_map(|sd| sd["rotation"].as_f64().map(|r| r.round() as i64))
}

fn parse_rate(s: Option<&str>) -> Option<f64> {
    let (n, d) = s?.split_once('/')?;
    let (n, d) = (n.parse::<f64>().ok()?, d.parse::<f64>().ok()?);
    (n > 0.0 && d > 0.0).then(|| n / d)
}

pub fn last_line(stderr: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stderr).lines().rev().map(str::trim).find(|l| !l.is_empty()).map(str::to_owned)
}

/// บรรทัดที่บอกสาเหตุจริง (FFmpeg มักพิมพ์สาเหตุไว้ก่อน แล้วตามด้วยข้อความสรุปกว้าง ๆ)
pub fn error_line(stderr: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stderr);
    let cause = text.lines().map(str::trim).find(|l| {
        let l = l.to_ascii_lowercase();
        ["no such file", "invalid", "not support", "driver", "permission", "could not", "error"].iter().any(|k| l.contains(k))
    });
    cause.map(str::to_owned).or_else(|| last_line(stderr))
}

/// ขนาดที่พอดีกรอบ max_w × max_h ตามอัตราส่วนภาพ (เป็นเลขคู่)
pub fn fit_size(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (max_w, max_h);
    }
    let s = (max_w as f64 / w as f64).min(max_h as f64 / h as f64);
    let even = |v: f64| (((v / 2.0).round() as u32) * 2).max(2);
    (even(w as f64 * s), even(h as f64 * s))
}

/// ภาพย่อหนึ่งภาพ คืน (กว้าง, สูง, พิกเซล RGBA)
pub fn thumbnail(ffmpeg: &Path, path: &Path, kind: MediaKind, info: &MediaInfo) -> Result<(u32, u32, Vec<u8>), String> {
    let (w, h) = fit_size(info.width, info.height, 192, 108);
    let mut cmd = command(ffmpeg);
    cmd.args(["-v", "error"]);
    if kind == MediaKind::Video {
        let at = info.duration.map(|d| (d * 0.25).min(1.0)).unwrap_or(0.0);
        cmd.args(["-ss", &format!("{at:.3}")]);
    }
    let out = cmd
        .arg("-i")
        .arg(path)
        .args(["-frames:v", "1", "-vf", &format!("scale={w}:{h}"), "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
        .output()
        .map_err(|e| e.to_string())?;
    let want = (w * h * 4) as usize;
    if out.stdout.len() < want {
        return Err(last_line(&out.stderr).unwrap_or_else(|| "ทำภาพย่อไม่ได้".to_owned()));
    }
    let mut px = out.stdout;
    px.truncate(want);
    Ok((w, h, px))
}

/// รูปคลื่นเสียง: ค่าความดังสูงสุด (0..1) ทุก ๆ 1/WAVE_PER_SEC วินาที
pub fn waveform(ffmpeg: &Path, path: &Path) -> Result<Vec<f32>, String> {
    let mut child = command(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", &WAVE_RATE.to_string(), "-f", "s16le", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let per_point = WAVE_RATE / WAVE_PER_SEC;
    let mut peaks = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let (mut count, mut peak) = (0usize, 0i32);
    let mut carry: Option<u8> = None;
    loop {
        let n = stdout.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let mut bytes = &buf[..n];
        let mut sample = |s: i16| {
            peak = peak.max((s as i32).abs());
            count += 1;
            if count == per_point {
                peaks.push(peak as f32 / 32768.0);
                count = 0;
                peak = 0;
            }
        };
        if let Some(lo) = carry.take() {
            sample(i16::from_le_bytes([lo, bytes[0]]));
            bytes = &bytes[1..];
        }
        let (pairs, rest) = bytes.as_chunks::<2>();
        for c in pairs {
            sample(i16::from_le_bytes(*c));
        }
        carry = rest.first().copied();
    }
    let _ = child.wait();
    if count > 0 {
        peaks.push(peak as f32 / 32768.0);
    }
    Ok(peaks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_rates() {
        assert_eq!(fit_size(1920, 1080, 192, 108), (192, 108));
        assert_eq!(fit_size(1080, 1920, 192, 108), (60, 108));
        assert_eq!(parse_rate(Some("30000/1001")).map(|r| (r * 100.0).round()), Some(2997.0));
        assert_eq!(parse_rate(Some("0/0")), None);
    }
}
