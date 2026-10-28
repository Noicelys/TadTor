//! เครื่องมือ MCP: รายชื่อ (ให้ AI อ่าน) และการทำงานจริงบนโปรแกรมที่เปิดอยู่
//!
//! ทุกคำสั่งที่แก้ไทม์ไลน์ผ่าน `TadTorApp::edit` ผู้ใช้จึงกด Ctrl+Z ย้อนได้เสมอ

use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::control::Request;
use super::Pending;
use crate::app::TadTorApp;
use crate::core::fx::{ClipFx, Effect, FilterPreset, Fit, TextStyle, TransitionKind, Voice};
use crate::core::media::MediaState;
use crate::core::timeline::{Clip, TrackKind};
use crate::engine::transcribe::{Language, TranscribeOptions, TranscribeState};

fn names<T: std::fmt::Debug>(all: &[T]) -> String {
    all.iter().map(|v| format!("{v:?}")).collect::<Vec<_>>().join(", ")
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

/// รายชื่อเครื่องมือทั้งหมด (คำอธิบายเป็นภาษาอังกฤษให้ AI อ่าน)
pub fn specs() -> Vec<Value> {
    let num = |d: &str| json!({ "type": "number", "description": d });
    let fx_help = format!(
        "Partial clip effects, only the keys to change. Keys: volume (0-2, 1=original), fade_in, fade_out (s), voice ({}), \
         pitch (semitones -12..12), bass, treble (dB -12..12), denoise (0-1), fit ({}), zoom (1=fit), pan_x, pan_y (-1..1), \
         rotate (quarter turns 0-3), flip_h, flip_v, background ([r,g,b,255]), filter ({}), filter_amount (0-1), brightness, \
         contrast, saturation, temperature, hue, tint, highlights, shadows (-1..1), sharpen, vignette, fade, grain (0-1), \
         effect ({}), effect_amount (0-2), transition (null or {{\"kind\": one of {}, \"dur\": seconds}}; main track, to the next clip).",
        names(Voice::ALL),
        names(Fit::ALL),
        names(FilterPreset::ALL),
        names(Effect::ALL),
        names(TransitionKind::ALL)
    );
    let text_help = "Partial text style, only the keys to change: content, font (installed font name, \"\" = default Thai font), \
         size (% of video height), bold, italic, color ([r,g,b,a]), opacity (0-1), align (Center, Left, Right), outline (% of size), \
         outline_color, shadow (bool), shadow_color, shadow_offset, shadow_angle (deg), background (bool), background_color, \
         box_padding, x, y (0-1, centre of the text), motion ({anim_in, in_dur, anim_out, out_dur, looping, loop_speed, ...}).";
    vec![
        tool(
            "get_timeline",
            "Project settings and every clip on the timeline (ids, track, start/end, source in/out, changed effects). Call this first.",
            json!({ "include_subtitles": { "type": "boolean", "description": "Also list subtitle lines (default false; see get_subtitles)" } }),
            &[],
        ),
        tool("list_media", "Files in the media library with duration, size and whether they are ready.", json!({}), &[]),
        tool(
            "import_media",
            "Add files (absolute Windows paths) to the media library. Video, audio and image files are supported.",
            json!({ "paths": { "type": "array", "items": { "type": "string" } } }),
            &["paths"],
        ),
        tool(
            "add_clip",
            "Put a file on the timeline (imports it first if needed). Video/images go to the main track (clips play back to back); \
             audio goes to the audio track at `start`.",
            json!({
                "path": { "type": "string", "description": "Absolute file path" },
                "index": { "type": "integer", "description": "Main track position (default: end)" },
                "start": num("Audio track start time in seconds (default: playhead)"),
            }),
            &["path"],
        ),
        tool(
            "split",
            "Split a clip at a timeline time. Without clip_id, splits the main-track clip under that time.",
            json!({ "time": num("Timeline time (s)"), "clip_id": { "type": "integer" } }),
            &["time"],
        ),
        tool(
            "delete_ranges",
            "Cut time ranges out of the whole project (ripple delete): everything after each range moves earlier on every track, \
             clips crossing a range are trimmed. Use timeline times from the current state; all ranges are applied in one undo step.",
            json!({ "ranges": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 },
                "description": "[[start, end], ...] in seconds" } }),
            &["ranges"],
        ),
        tool(
            "delete_clips",
            "Remove clips by id (main track closes the gap).",
            json!({ "ids": { "type": "array", "items": { "type": "integer" } } }),
            &["ids"],
        ),
        tool(
            "trim_clip",
            "Trim a clip: remove seconds from its head and/or tail (negative values extend it back if source allows).",
            json!({ "id": { "type": "integer" }, "trim_start": num("Seconds to remove from the start"), "trim_end": num("Seconds to remove from the end") }),
            &["id"],
        ),
        tool(
            "move_clip",
            "Move a clip: main track by `index`, other tracks to time `start` (snaps to the nearest free spot).",
            json!({ "id": { "type": "integer" }, "index": { "type": "integer" }, "start": num("Seconds") }),
            &["id"],
        ),
        tool(
            "set_clip_properties",
            "Change a clip's effects/audio/layout (`fx`), text style (`text`, text and subtitle clips) or duration (images/text only).",
            json!({
                "id": { "type": "integer" },
                "fx": { "type": "object", "description": fx_help },
                "text": { "type": "object", "description": text_help },
                "duration": num("New duration in seconds (images and text only)"),
            }),
            &["id"],
        ),
        tool(
            "add_text",
            "Add a text overlay on the text track.",
            json!({
                "text": { "type": "string" },
                "start": num("Seconds"),
                "duration": num("Seconds (default 3)"),
                "preset": { "type": "string", "description": format!("Style preset name: {}", TextStyle::presets().iter().map(|p| p.0).collect::<Vec<_>>().join(", ")) },
                "style": { "type": "object", "description": text_help },
            }),
            &["text", "start"],
        ),
        tool("get_subtitles", "All subtitle lines with ids and times (use these times to find what to cut).", json!({}), &[]),
        tool(
            "set_subtitles",
            "Replace subtitles with the given lines (all of them, or only inside [from, until)). Keeps the style of existing subtitles.",
            json!({
                "items": { "type": "array", "items": { "type": "object", "properties": {
                    "start": { "type": "number" }, "end": { "type": "number" }, "text": { "type": "string" } }, "required": ["start", "end", "text"] } },
                "from": num("Optional range start"),
                "until": num("Optional range end"),
            }),
            &["items"],
        ),
        tool(
            "auto_subtitles",
            "Transcribe speech into subtitles with local Whisper (runs on CPU, roughly real time; the first run downloads ~580 MB). \
             Waits until finished and returns the lines.",
            json!({
                "language": { "type": "string", "description": "th (default), en, ja, zh, ko or auto" },
                "from": num("Range start (default 0)"),
                "until": num("Range end (default: end of project)"),
                "max_chars": { "type": "integer", "description": "Max characters per subtitle line, 0 = no limit (default 42)" },
                "prompt": { "type": "string", "description": "Names/terms spoken in the video, helps spelling" },
                "include_music": { "type": "boolean", "description": "Also listen to the audio track (default false)" },
            }),
            &[],
        ),
        tool(
            "detect_silence",
            "Find pauses (silence) in the project's audio. Returns [[start, end], ...] in timeline seconds.",
            json!({
                "min_silence": num("Shortest pause to report, seconds (default 0.7)"),
                "include_music": { "type": "boolean", "description": "Listen to the audio track too (default false: only clip audio)" },
            }),
            &[],
        ),
        tool(
            "remove_silence",
            "Cut out pauses automatically (like jump cuts), keeping a little padding around speech. One undo step.",
            json!({
                "min_silence": num("Only cut pauses at least this long, seconds (default 0.7)"),
                "padding": num("Silence to keep before/after speech, seconds (default 0.15)"),
                "include_music": { "type": "boolean" },
            }),
            &[],
        ),
        tool("seek", "Move the playhead.", json!({ "time": num("Seconds") }), &["time"]),
        tool("undo", "Undo the last edit.", json!({}), &[]),
        tool("redo", "Redo.", json!({}), &[]),
        tool(
            "save_project",
            "Save the project (.tadtor). `path` is required if it was never saved.",
            json!({ "path": { "type": "string" } }),
            &[],
        ),
        tool(
            "export_video",
            "Start exporting an MP4 (runs in the background; check get_status).",
            json!({
                "path": { "type": "string", "description": "Output .mp4 path" },
                "resolution": { "type": "integer", "description": "Short side in pixels: 480, 720, 1080 (default project), 1440, 2160" },
                "fps": { "type": "integer" },
                "quality": { "type": "string", "description": "high (default), standard, small" },
                "codec": { "type": "string", "description": "h264 (default) or h265" },
            }),
            &["path"],
        ),
        tool("get_status", "Export / transcription progress, unsaved changes, playhead.", json!({}), &[]),
    ]
}

// ---------------- อ่านค่าจาก args ----------------

fn num(args: &Value, key: &str) -> Result<f64, String> {
    args[key].as_f64().ok_or_else(|| format!("ต้องระบุ {key} เป็นตัวเลข"))
}

fn opt_num(args: &Value, key: &str) -> Option<f64> {
    args[key].as_f64()
}

fn id_of(args: &Value, key: &str) -> Result<u64, String> {
    args[key].as_u64().ok_or_else(|| format!("ต้องระบุ {key}"))
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// แก้ค่าบางส่วนของ struct ด้วย JSON (ชื่อที่ไม่รู้จัก = ผิด)
fn merge<T: Serialize + DeserializeOwned>(cur: &T, patch: &Value) -> Result<T, String> {
    let mut v = serde_json::to_value(cur).map_err(|e| e.to_string())?;
    let (Some(obj), Some(p)) = (v.as_object_mut(), patch.as_object()) else {
        return Err("ค่าที่จะแก้ต้องเป็น object".to_owned());
    };
    for (k, val) in p {
        if !obj.contains_key(k) {
            return Err(format!("ไม่รู้จักค่า '{k}'"));
        }
        obj.insert(k.clone(), val.clone());
    }
    serde_json::from_value(v).map_err(|e| format!("ค่าไม่ถูกต้อง: {e}"))
}

/// ปัดทศนิยม (ค่า f32 กลายเป็นเลขยาว ๆ อย่าง 0.800000011920929 ตอนแปลงเป็น JSON)
fn tidy(v: Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => json!((n.as_f64().unwrap_or(0.0) * 10_000.0).round() / 10_000.0),
        Value::Array(a) => Value::Array(a.into_iter().map(tidy).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, tidy(v))).collect()),
        other => other,
    }
}

/// ค่าที่ต่างจากค่าเริ่มต้น (ให้ AI อ่านง่าย)
fn diff<T: Serialize + Default>(cur: &T) -> Value {
    let (a, b) = (serde_json::to_value(cur).unwrap_or_default(), serde_json::to_value(T::default()).unwrap_or_default());
    let mut out = Map::new();
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        for (k, v) in a {
            if b.get(k) != Some(v) {
                out.insert(k.clone(), tidy(v.clone()));
            }
        }
    }
    Value::Object(out)
}

fn track_name(k: TrackKind) -> &'static str {
    match k {
        TrackKind::Main => "main",
        TrackKind::Text => "text",
        TrackKind::Subtitle => "subtitle",
        TrackKind::Audio => "audio",
    }
}

fn clip_json(c: &Clip, kind: TrackKind, index: usize) -> Value {
    let mut v = json!({
        "id": c.id,
        "track": track_name(kind),
        "index": index,
        "start": r3(c.start),
        "end": r3(c.end()),
        "duration": r3(c.dur),
    });
    match &c.text {
        Some(t) => {
            v["text"] = json!(t.content);
            let style = diff(&TextStyle { content: TextStyle::default().content, ..t.clone() });
            if style.as_object().is_some_and(|o| !o.is_empty()) {
                v["style"] = style;
            }
        }
        None => {
            v["name"] = json!(c.name);
            v["file"] = json!(c.path.display().to_string());
            v["kind"] = json!(format!("{:?}", c.kind));
            if c.src_len.is_some() {
                v["source_in"] = json!(r3(c.src_in));
                v["source_out"] = json!(r3(c.src_in + c.dur));
            }
            v["has_audio"] = json!(c.has_audio);
        }
    }
    let fx = diff(&c.fx);
    if fx.as_object().is_some_and(|o| !o.is_empty()) {
        v["fx"] = fx;
    }
    v
}

fn subtitles_json(app: &TadTorApp) -> Value {
    json!(app
        .timeline
        .subtitles
        .iter()
        .map(|c| json!({ "id": c.id, "start": r3(c.start), "end": r3(c.end()), "text": c.title() }))
        .collect::<Vec<_>>())
}

// ---------------- ทำงาน ----------------

/// ผลของการรันคำสั่ง: เสร็จแล้ว หรือต้องรอ (ยังไม่ตอบ เก็บไว้ทำต่อรอบหน้า)
pub enum Outcome {
    Done(Result<Value, String>),
    Wait(Pending),
}

use Outcome::Done;

pub fn run(app: &mut TadTorApp, ctx: &egui::Context, req: &Request) -> Outcome {
    let a = &req.args;
    match req.tool.as_str() {
        "get_timeline" => Done(Ok(timeline(app, a["include_subtitles"].as_bool().unwrap_or(false)))),
        "list_media" => Done(Ok(media(app))),
        "import_media" => Done(import(app, a)),
        "add_clip" => add_clip(app, ctx, a),
        "split" => Done(split(app, a)),
        "delete_ranges" => Done(delete_ranges(app, ctx, a)),
        "delete_clips" => Done(delete_clips(app, a)),
        "trim_clip" => Done(trim(app, a)),
        "move_clip" => Done(move_clip(app, a)),
        "set_clip_properties" => Done(set_props(app, a)),
        "add_text" => Done(add_text(app, a)),
        "get_subtitles" => Done(Ok(subtitles_json(app))),
        "set_subtitles" => Done(set_subtitles(app, a)),
        "auto_subtitles" => auto_subtitles(app, ctx, a),
        "detect_silence" | "remove_silence" => silence(app, req.tool == "remove_silence", a),
        "seek" => Done(num(a, "time").map(|t| {
            app.seek(t);
            json!({ "playhead": r3(app.playhead) })
        })),
        "undo" => {
            app.undo();
            Done(Ok(json!({ "duration": r3(app.timeline.duration()) })))
        }
        "redo" => {
            app.redo();
            Done(Ok(json!({ "duration": r3(app.timeline.duration()) })))
        }
        "save_project" => Done(save(app, ctx, a)),
        "export_video" => Done(export(app, ctx, a)),
        "get_status" => Done(Ok(status(app))),
        other => Done(Err(format!("ไม่รู้จักคำสั่ง {other}"))),
    }
}

fn timeline(app: &TadTorApp, include_subs: bool) -> Value {
    let tl = &app.timeline;
    let (w, h) = app.project.resolution();
    let track = |k: TrackKind| tl.track(k).iter().enumerate().map(|(i, c)| clip_json(c, k, i)).collect::<Vec<_>>();
    let mut v = json!({
        "project": { "name": app.project.name, "aspect": app.project.aspect.label(), "width": w, "height": h, "fps": app.project.fps },
        "duration": r3(tl.duration()),
        "playhead": r3(app.playhead),
        "selected_clip": app.selected_clip,
        "main": track(TrackKind::Main),
        "text": track(TrackKind::Text),
        "audio": track(TrackKind::Audio),
        "subtitle_count": tl.subtitles.len(),
        "muted": { "main": tl.main_muted, "audio": tl.audio_muted },
    });
    if include_subs {
        v["subtitles"] = subtitles_json(app);
    }
    v
}

fn media(app: &TadTorApp) -> Value {
    json!(app
        .library
        .items
        .iter()
        .map(|it| {
            let mut v = json!({ "path": it.path.display().to_string(), "name": it.name, "kind": format!("{:?}", it.kind) });
            match &it.state {
                MediaState::Probing => v["status"] = json!("loading"),
                MediaState::Failed(e) => v["status"] = json!(format!("error: {e}")),
                MediaState::Ready(i) => {
                    v["status"] = json!("ready");
                    v["duration"] = json!(i.duration.map(r3));
                    if i.has_video {
                        v["width"] = json!(i.width);
                        v["height"] = json!(i.height);
                    }
                    v["has_audio"] = json!(i.has_audio);
                }
            }
            v
        })
        .collect::<Vec<_>>())
}

fn paths_of(a: &Value) -> Result<Vec<PathBuf>, String> {
    let list = a["paths"].as_array().ok_or("ต้องระบุ paths")?;
    let paths: Vec<PathBuf> = list.iter().filter_map(|p| p.as_str()).map(PathBuf::from).collect();
    if let Some(missing) = paths.iter().find(|p| !p.is_file()) {
        return Err(format!("ไม่พบไฟล์ {}", missing.display()));
    }
    Ok(paths)
}

fn import(app: &mut TadTorApp, a: &Value) -> Result<Value, String> {
    let paths = paths_of(a)?;
    let added = app.add_media_quiet(paths);
    Ok(json!({ "added": added, "note": "files are being read; list_media shows when they are ready" }))
}

fn add_clip(app: &mut TadTorApp, ctx: &egui::Context, a: &Value) -> Outcome {
    let Some(path) = a["path"].as_str().map(PathBuf::from) else { return Done(Err("ต้องระบุ path".to_owned())) };
    if app.library.get(&path).is_none() {
        if !path.is_file() {
            return Done(Err(format!("ไม่พบไฟล์ {}", path.display())));
        }
        if app.add_media_quiet(vec![path.clone()]) == 0 {
            return Done(Err("ไฟล์นี้ไม่รองรับ".to_owned()));
        }
    }
    let item = app.library.get(&path).unwrap();
    match &item.state {
        MediaState::Probing => Outcome::Wait(Pending::Retry),
        MediaState::Failed(e) => Done(Err(format!("ใช้ไฟล์นี้ไม่ได้: {e}"))),
        MediaState::Ready(_) => {
            let before: Vec<u64> = app.timeline.all_clips().map(|c| c.id).collect();
            let index = a["index"].as_u64().map(|i| (i as usize).min(app.timeline.main.len()));
            app.add_to_timeline(ctx, &path, index, opt_num(a, "start"));
            let new = app.timeline.all_clips().find(|c| !before.contains(&c.id)).cloned();
            match new {
                Some(c) => {
                    let (kind, i) = app.timeline.find(c.id).unwrap();
                    app.notify(ctx, format!("AI: ใส่ {} ลงไทม์ไลน์", c.name));
                    Done(Ok(clip_json(&c, kind, i)))
                }
                None => Done(Err("ใส่คลิปไม่สำเร็จ".to_owned())),
            }
        }
    }
}

fn split(app: &mut TadTorApp, a: &Value) -> Result<Value, String> {
    let t = num(a, "time")?;
    let id = match a["clip_id"].as_u64() {
        Some(id) => id,
        None => app.timeline.clip_at(TrackKind::Main, t).filter(|c| c.contains(t)).map(|c| c.id).ok_or("ไม่มีคลิปแทร็กหลักตรงเวลานี้")?,
    };
    let right = app.edit(|tl| tl.split(id, t)).ok_or("แยกไม่ได้ (ต้องอยู่ในคลิป และแต่ละชิ้นยาวอย่างน้อย 0.1 วิ)")?;
    Ok(json!({ "left_id": id, "right_id": right }))
}

fn delete_ranges(app: &mut TadTorApp, ctx: &egui::Context, a: &Value) -> Result<Value, String> {
    let list = a["ranges"].as_array().ok_or("ต้องระบุ ranges")?;
    let mut ranges: Vec<(f64, f64)> = Vec::new();
    for r in list {
        let (Some(s), Some(e)) = (r[0].as_f64(), r[1].as_f64()) else {
            return Err("แต่ละช่วงต้องเป็น [เริ่ม, จบ]".to_owned());
        };
        if e > s {
            ranges.push((s, e));
        }
    }
    let before = app.timeline.duration();
    let n = app.edit(|tl| delete_merged(tl, ranges));
    let after = app.timeline.duration();
    app.stop_playback();
    app.seek(app.playhead.min(after));
    app.notify(ctx, format!("AI: ตัดออก {n} ช่วง ({:.1} วิ)", before - after));
    Ok(json!({ "ranges_cut": n, "seconds_removed": r3(before - after), "duration": r3(after) }))
}

/// ตัดหลายช่วง: รวมช่วงที่ทับกันก่อน แล้วตัดจากท้ายมาหน้า (เวลาของช่วงที่เหลือจึงไม่เลื่อน)
fn delete_merged(tl: &mut crate::core::timeline::Timeline, mut ranges: Vec<(f64, f64)>) -> usize {
    ranges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in ranges {
        match merged.last_mut() {
            Some(l) if s <= l.1 => l.1 = l.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    merged.iter().rev().filter(|(s, e)| tl.delete_range(*s, *e)).count()
}

fn delete_clips(app: &mut TadTorApp, a: &Value) -> Result<Value, String> {
    let ids: Vec<u64> = a["ids"].as_array().ok_or("ต้องระบุ ids")?.iter().filter_map(Value::as_u64).collect();
    let removed = app.edit(|tl| ids.iter().filter(|&&id| tl.remove(id)).count());
    if app.selected_clip.is_some_and(|id| ids.contains(&id)) {
        app.selected_clip = None;
    }
    Ok(json!({ "removed": removed, "duration": r3(app.timeline.duration()) }))
}

fn trim(app: &mut TadTorApp, a: &Value) -> Result<Value, String> {
    let id = id_of(a, "id")?;
    app.timeline.clip(id).ok_or("ไม่พบคลิปนี้")?;
    let (head, tail) = (opt_num(a, "trim_start").unwrap_or(0.0), opt_num(a, "trim_end").unwrap_or(0.0));
    let (h, t) = app.edit(|tl| (tl.trim_head(id, head), -tl.trim_tail(id, -tail)));
    let c = app.timeline.clip(id).cloned().ok_or("ไม่พบคลิปนี้")?;
    let (kind, i) = app.timeline.find(id).unwrap();
    Ok(json!({ "trimmed_start": r3(h), "trimmed_end": r3(t), "clip": clip_json(&c, kind, i) }))
}

fn move_clip(app: &mut TadTorApp, a: &Value) -> Result<Value, String> {
    let id = id_of(a, "id")?;
    let (kind, _) = app.timeline.find(id).ok_or("ไม่พบคลิปนี้")?;
    if kind == TrackKind::Main {
        let to = a["index"].as_u64().ok_or("คลิปแทร็กหลักต้องระบุ index")? as usize;
        app.edit(|tl| tl.move_main(id, to.min(tl.main.len().saturating_sub(1))));
    } else {
        let at = num(a, "start")?;
        app.edit(|tl| tl.move_free(id, at));
    }
    let (kind, i) = app.timeline.find(id).unwrap();
    Ok(clip_json(app.timeline.clip(id).unwrap(), kind, i))
}

fn set_props(app: &mut TadTorApp, a: &Value) -> Result<Value, String> {
    let id = id_of(a, "id")?;
    let c = app.timeline.clip(id).cloned().ok_or("ไม่พบคลิปนี้")?;
    let fx: ClipFx = if a["fx"].is_object() { merge(&c.fx, &a["fx"])? } else { c.fx.clone() };
    let text = match (&c.text, &a["text"]) {
        (Some(t), p) if p.is_object() => Some(merge(t, p)?),
        (None, p) if p.is_object() => return Err("คลิปนี้ไม่ใช่ข้อความ".to_owned()),
        (t, _) => t.clone(),
    };
    let dur = opt_num(a, "duration");
    if dur.is_some() && c.src_len.is_some() {
        return Err("ตั้งความยาวได้เฉพาะรูปภาพ/ข้อความ (วิดีโอ/เสียงใช้ trim_clip)".to_owned());
    }
    app.edit(|tl| {
        tl.update_clip(id, |c| {
            c.fx = fx;
            c.text = text;
        });
        if let Some(d) = dur {
            tl.set_duration(id, d.max(0.1));
        }
        tl.pack();
    });
    let (kind, i) = app.timeline.find(id).unwrap();
    Ok(clip_json(app.timeline.clip(id).unwrap(), kind, i))
}

fn add_text(app: &mut TadTorApp, a: &Value) -> Result<Value, String> {
    let content = a["text"].as_str().ok_or("ต้องระบุ text")?.to_owned();
    let start = num(a, "start")?;
    let dur = opt_num(a, "duration").unwrap_or(3.0).max(0.1);
    let mut style = match a["preset"].as_str() {
        Some(name) => TextStyle::presets().into_iter().find(|p| p.0 == name).map(|p| p.1).ok_or(format!("ไม่มีสไตล์ชื่อ {name}"))?,
        None => TextStyle::default(),
    };
    if a["style"].is_object() {
        style = merge(&style, &a["style"])?;
    }
    style.content = content;
    let id = app.edit(|tl| {
        let c = tl.new_text_clip(style, dur);
        tl.place(TrackKind::Text, c, start)
    });
    let (kind, i) = app.timeline.find(id).unwrap();
    Ok(clip_json(app.timeline.clip(id).unwrap(), kind, i))
}

fn set_subtitles(app: &mut TadTorApp, a: &Value) -> Result<Value, String> {
    let mut items = Vec::new();
    for it in a["items"].as_array().ok_or("ต้องระบุ items")? {
        let (Some(s), Some(e), Some(t)) = (it["start"].as_f64(), it["end"].as_f64(), it["text"].as_str()) else {
            return Err("แต่ละบรรทัดต้องมี start, end, text".to_owned());
        };
        items.push((s, e, t.to_owned()));
    }
    let style = app.timeline.subtitles.first().and_then(|c| c.text.clone()).unwrap_or_else(|| TextStyle::subtitle(String::new()));
    let from = opt_num(a, "from").unwrap_or(0.0);
    let until = opt_num(a, "until").unwrap_or(f64::MAX);
    app.edit(|tl| tl.replace_subtitles_in(from, until, items, &style));
    Ok(json!({ "subtitle_count": app.timeline.subtitles.len() }))
}

fn auto_subtitles(app: &mut TadTorApp, ctx: &egui::Context, a: &Value) -> Outcome {
    if app.transcribe_job.is_some() {
        return Done(Err("กำลังสร้างซับอยู่แล้ว รอให้เสร็จก่อน".to_owned()));
    }
    let language = match a["language"].as_str().unwrap_or("th") {
        "th" => Language::Thai,
        "en" => Language::English,
        "ja" => Language::Japanese,
        "zh" => Language::Chinese,
        "ko" => Language::Korean,
        "auto" => Language::Auto,
        other => return Done(Err(format!("ไม่รองรับภาษา {other}"))),
    };
    let total = app.timeline.duration();
    let opts = TranscribeOptions {
        language,
        include_music: a["include_music"].as_bool().unwrap_or(false),
        from: opt_num(a, "from").unwrap_or(0.0).clamp(0.0, total),
        until: opt_num(a, "until").unwrap_or(total).clamp(0.0, total),
        max_len: a["max_chars"].as_u64().unwrap_or(app.settings.subtitles.max_len as u64) as u32,
        prompt: a["prompt"].as_str().unwrap_or_default().to_owned(),
    };
    let presets = TextStyle::presets();
    let style = presets.get(app.settings.subtitles.style).map_or_else(|| TextStyle::subtitle(String::new()), |p| p.1.clone());
    match crate::ui::subtitle_dialog::start_job(app, ctx, opts, style) {
        Ok(()) => Outcome::Wait(Pending::Subtitles),
        Err(e) => Done(Err(e)),
    }
}

/// เช็กงานถอดเสียงที่สั่งจาก MCP (ผลถูกใส่ลงไทม์ไลน์โดยหน้าต่างความคืบหน้าแล้ว)
pub fn subtitles_result(app: &mut TadTorApp) -> Option<Result<Value, String>> {
    let Some(job) = &app.transcribe_job else { return Some(Ok(subtitles_json(app))) };
    match job.state() {
        TranscribeState::Done(items) => {
            crate::ui::subtitle_dialog::apply_result(app, &items);
            app.transcribe_job = None;
            app.subtitle_dialog = None;
            Some(Ok(json!({ "subtitle_count": app.timeline.subtitles.len(), "subtitles": subtitles_json(app) })))
        }
        TranscribeState::Failed(e) => {
            app.transcribe_job = None;
            app.subtitle_dialog = None;
            Some(Err(e))
        }
        TranscribeState::Cancelled => {
            app.transcribe_job = None;
            app.subtitle_dialog = None;
            Some(Err("ผู้ใช้ยกเลิกการสร้างซับ".to_owned()))
        }
        _ => None,
    }
}

fn silence(app: &mut TadTorApp, remove: bool, a: &Value) -> Outcome {
    let Some(info) = app.ffmpeg.info() else { return Done(Err("ไม่พบ FFmpeg".to_owned())) };
    let total = app.timeline.duration();
    if total < 0.2 {
        return Done(Err("ไทม์ไลน์ยังว่าง".to_owned()));
    }
    let min = opt_num(a, "min_silence").unwrap_or(0.7).max(0.1);
    let pad = opt_num(a, "padding").unwrap_or(0.15).clamp(0.0, 1.0);
    let mut tl = app.timeline.clone();
    tl.main_muted = false;
    tl.audio_muted = !a["include_music"].as_bool().unwrap_or(false);
    let (tx, rx) = mpsc::channel();
    let ctx_ffmpeg = info.ffmpeg.clone();
    std::thread::spawn(move || {
        let r = crate::engine::transcribe::render_audio(&ctx_ffmpeg, &tl, 0.0, total).map(|(rate, samples)| {
            let regions = crate::core::speech::speech_regions(&samples, rate, Default::default());
            crate::core::speech::silences(&regions, total, min)
        });
        let _ = tx.send(r);
    });
    Outcome::Wait(Pending::Silence { rx, remove, pad })
}

/// ผลหาช่วงเงียบเสร็จแล้ว: รายงาน หรือตัดออก
pub fn silence_result(app: &mut TadTorApp, ctx: &egui::Context, found: Vec<(f64, f64)>, remove: bool, pad: f64) -> Result<Value, String> {
    let list: Vec<Value> = found.iter().map(|(s, e)| json!([r3(*s), r3(*e)])).collect();
    if !remove {
        let total: f64 = found.iter().map(|(s, e)| e - s).sum();
        return Ok(json!({ "silences": list, "count": found.len(), "total_seconds": r3(total) }));
    }
    let end = app.timeline.duration();
    // เก็บเสียงเงียบไว้หัวท้ายช่วงพูดนิดหน่อย (ช่วงเงียบตอนต้น/ท้ายงานตัดได้เต็ม)
    let cuts: Vec<(f64, f64)> = found
        .iter()
        .map(|&(s, e)| (if s <= 0.0 { 0.0 } else { s + pad }, if e >= end - 1e-6 { e } else { e - pad }))
        .filter(|(s, e)| e - s >= 0.05)
        .collect();
    let before = app.timeline.duration();
    let n = app.edit(|tl| delete_merged(tl, cuts));
    let after = app.timeline.duration();
    app.stop_playback();
    app.seek(app.playhead.min(after));
    app.notify(ctx, format!("AI: ตัดช่วงเงียบ {n} ช่วง ({:.1} วิ)", before - after));
    Ok(json!({ "ranges_cut": n, "seconds_removed": r3(before - after), "duration": r3(after) }))
}

fn save(app: &mut TadTorApp, ctx: &egui::Context, a: &Value) -> Result<Value, String> {
    if let Some(p) = a["path"].as_str() {
        let mut p = PathBuf::from(p);
        if p.extension().is_none_or(|e| e != crate::core::project_file::EXTENSION) {
            p.set_extension(crate::core::project_file::EXTENSION);
        }
        app.project_path = Some(p);
    }
    if app.project_path.is_none() {
        return Err("โปรเจกต์นี้ยังไม่เคยบันทึก ต้องระบุ path".to_owned());
    }
    if app.save(ctx, false) {
        Ok(json!({ "saved": app.project_path.as_ref().map(|p| p.display().to_string()) }))
    } else {
        Err("บันทึกไม่สำเร็จ".to_owned())
    }
}

fn export(app: &mut TadTorApp, ctx: &egui::Context, a: &Value) -> Result<Value, String> {
    use crate::engine::export::{Codec, Encoder, ExportJob, ExportSettings, Quality, Speed};
    if app.export_job.is_some() {
        return Err("กำลังส่งออกอยู่ รอให้เสร็จก่อน".to_owned());
    }
    if app.timeline.is_empty() {
        return Err("ไทม์ไลน์ยังว่าง".to_owned());
    }
    let info = app.ffmpeg.info().ok_or("ไม่พบ FFmpeg")?;
    let mut path = PathBuf::from(a["path"].as_str().ok_or("ต้องระบุ path")?);
    if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("mp4")) {
        path.set_extension("mp4");
    }
    let short = a["resolution"].as_u64().map_or(app.project.quality, |v| v as u32);
    let (width, height) = app.project.aspect.resolution(short);
    let quality = match a["quality"].as_str().unwrap_or("high") {
        "standard" => Quality::Standard,
        "small" => Quality::Small,
        _ => Quality::High,
    };
    let codec = if a["codec"].as_str() == Some("h265") { Codec::H265 } else { Codec::H264 };
    let nvenc = if codec == Codec::H265 { info.nvenc_hevc } else { info.nvenc_h264 };
    let settings = ExportSettings {
        path: path.clone(),
        video: crate::ffmpeg::graph::VideoSpec { width, height, fps: a["fps"].as_u64().map_or(app.project.fps, |v| v as u32) },
        encoder: if nvenc { Encoder::Nvenc } else { Encoder::Cpu },
        codec,
        quality,
        crf: 20,
        speed: Speed::Balanced,
        audio_bitrate: 192,
    };
    app.stop_playback();
    app.export_dialog = None;
    app.export_job = Some(ExportJob::start(&info.ffmpeg, &app.timeline, settings, ctx.clone()));
    Ok(json!({ "started": path.display().to_string(), "size": [width, height], "note": "call get_status for progress" }))
}

fn status(app: &TadTorApp) -> Value {
    use crate::engine::export::ExportState;
    let export = app.export_job.as_ref().map(|j| match j.state() {
        ExportState::Running { progress, .. } => json!({ "state": "running", "progress": progress }),
        ExportState::Done { path, seconds } => json!({ "state": "done", "path": path.display().to_string(), "seconds": r3(seconds) }),
        ExportState::Failed(e) => json!({ "state": "failed", "error": e }),
        ExportState::Cancelled => json!({ "state": "cancelled" }),
    });
    let transcribe = app.transcribe_job.as_ref().map(|j| match j.state() {
        TranscribeState::Downloading { what, progress } => json!({ "state": format!("downloading {what}"), "progress": progress }),
        TranscribeState::Rendering => json!({ "state": "preparing audio" }),
        TranscribeState::Running { progress } => json!({ "state": "transcribing", "progress": progress }),
        TranscribeState::Done(v) => json!({ "state": "done", "lines": v.len() }),
        TranscribeState::Failed(e) => json!({ "state": "failed", "error": e }),
        TranscribeState::Cancelled => json!({ "state": "cancelled" }),
    });
    json!({
        "export": export,
        "transcribe": transcribe,
        "unsaved_changes": app.dirty,
        "project_path": app.project_path.as_ref().map(|p| p.display().to_string()),
        "duration": r3(app.timeline.duration()),
        "playhead": r3(app.playhead),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_are_valid() {
        let s = specs();
        assert!(s.len() > 15);
        for t in &s {
            assert!(t["name"].is_string() && t["inputSchema"]["type"] == "object", "{t}");
        }
    }

    #[test]
    fn merge_rejects_unknown_keys() {
        let fx = merge(&ClipFx::default(), &json!({ "volume": 0.5, "filter": "Vivid" })).unwrap();
        assert_eq!((fx.volume, fx.filter), (0.5, FilterPreset::Vivid));
        assert!(merge(&ClipFx::default(), &json!({ "volum": 0.5 })).is_err());
        assert!(merge(&ClipFx::default(), &json!({ "filter": "Nope" })).is_err());
        assert_eq!(diff(&fx), json!({ "volume": 0.5, "filter": "Vivid" }));
    }

    #[test]
    fn merged_ranges_cut_back_to_front() {
        let mut tl = crate::core::timeline::Timeline::default();
        let info = crate::core::media::MediaInfo {
            duration: Some(20.0),
            width: 1920,
            height: 1080,
            fps: Some(30.0),
            has_video: true,
            has_audio: true,
        };
        let c = tl.new_clip("a.mp4".into(), "a".into(), crate::core::media::MediaKind::Video, &info);
        tl.insert_main(0, c);
        // 2..4 และ 3..5 ทับกัน = ตัด 2..5, แล้วตัด 10..12 (เวลาเดิม)
        assert_eq!(delete_merged(&mut tl, vec![(10.0, 12.0), (2.0, 4.0), (3.0, 5.0)]), 2);
        assert_eq!(tl.duration(), 15.0);
        let pieces: Vec<(f64, f64)> = tl.main.iter().map(|c| (c.src_in, c.dur)).collect();
        assert_eq!(pieces, [(0.0, 2.0), (5.0, 5.0), (12.0, 8.0)]);
    }
}
