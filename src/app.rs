use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;

use crate::core::fx::TextStyle;
use crate::core::history::History;
use crate::core::media::{MediaKind, MediaLibrary, MediaState};
use crate::core::project::Project;
use crate::core::project_file;
use crate::core::subtitles;
use crate::core::timeline::{Clip, Timeline, TrackKind, TEXT_DEFAULT};
use crate::engine::audio::AudioOut;
use crate::engine::export::ExportJob;
use crate::engine::jobs::{MediaEvent, MediaJobs};
use crate::engine::player::{FrameGrabber, Player};
use crate::engine::transcribe::TranscribeJob;
use crate::ffmpeg::graph::VideoSpec;
use crate::ffmpeg::FfmpegProbe;
use crate::{icons, theme, ui};

/// แท็บด้านบนของแผงซ้าย (เหมือนแถบเครื่องมือของโปรแกรมตัดต่อ)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeftTab {
    Media,
    Audio,
    Text,
    Subtitles,
    Effects,
    Transitions,
    Filters,
    Adjust,
}

impl LeftTab {
    pub const ALL: [LeftTab; 8] = [
        LeftTab::Media,
        LeftTab::Audio,
        LeftTab::Text,
        LeftTab::Subtitles,
        LeftTab::Effects,
        LeftTab::Transitions,
        LeftTab::Filters,
        LeftTab::Adjust,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            LeftTab::Media => icons::FILM_STRIP,
            LeftTab::Audio => icons::MUSIC_NOTES,
            LeftTab::Text => icons::TEXT_T,
            LeftTab::Subtitles => icons::SUBTITLES,
            LeftTab::Effects => icons::SPARKLE,
            LeftTab::Transitions => icons::INTERSECT,
            LeftTab::Filters => icons::CIRCLE_HALF,
            LeftTab::Adjust => icons::SLIDERS_HORIZONTAL,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LeftTab::Media => "สื่อ",
            LeftTab::Audio => "เสียง",
            LeftTab::Text => "ข้อความ",
            LeftTab::Subtitles => "ซับไตเติล",
            LeftTab::Effects => "เอฟเฟกต์",
            LeftTab::Transitions => "ทรานซิชัน",
            LeftTab::Filters => "ฟิลเตอร์",
            LeftTab::Adjust => "ปรับแต่ง",
        }
    }
}

/// งานที่รอให้ผู้ใช้ตอบก่อนว่าจะบันทึกงานเดิมไหม
#[derive(Clone, Debug)]
pub enum PendingAction {
    Close,
    NewProject,
    OpenProject,
    /// เปิดงานจากรายการงานล่าสุด
    OpenPath(PathBuf),
}

/// ประเภทคลิปที่คำสั่งต้องการ
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    /// คลิปภาพ/วิดีโอในแทร็กหลัก
    Visual,
    /// คลิปที่มีเสียง
    Sound,
    Text,
}

/// สิ่งที่ลากจากคลังสื่อ (ใช้กับระบบลากวางของ egui)
#[derive(Clone, Debug)]
pub struct MediaDrag {
    pub path: PathBuf,
}

pub struct TadTorApp {
    pub project: Project,
    pub library: MediaLibrary,
    pub ffmpeg: FfmpegProbe,
    jobs: MediaJobs,

    pub timeline: Timeline,
    pub history: History,
    /// เพิ่มทุกครั้งที่ไทม์ไลน์เปลี่ยน (ใช้รู้ว่าต้องเรนเดอร์ภาพพรีวิวใหม่)
    pub timeline_rev: u64,
    /// สำเนาไทม์ไลน์ตอนเริ่มลากคลิป (ลากเสร็จค่อยเก็บลง Undo ทีเดียว)
    pub drag_origin: Option<Timeline>,

    pub thumbs: HashMap<PathBuf, egui::TextureHandle>,
    /// โลโก้โปรแกรม (assets/icon-256.png) ใช้ที่แถบบน
    pub logo: Option<egui::TextureHandle>,
    pub waveforms: HashMap<PathBuf, Arc<Vec<f32>>>,

    pub left_tab: LeftTab,
    pub selected_media: Option<PathBuf>,
    pub selected_clip: Option<u64>,

    /// ตำแหน่งหัวเล่น (วินาที)
    pub playhead: f64,
    pub playing: bool,
    /// ความละเอียดไทม์ไลน์: กี่พิกเซลต่อ 1 วินาที
    pub px_per_sec: f32,

    player: Option<Player>,
    audio_out: Option<AudioOut>,
    audio_error: Option<String>,
    grabber: FrameGrabber,
    grab_key: Option<(u64, i64, VideoSpec)>,
    grab_id: u64,
    pub preview_tex: Option<egui::TextureHandle>,

    pub project_path: Option<PathBuf>,
    pub dirty: bool,
    pub pending: Option<PendingAction>,
    allow_close: bool,

    pub export_dialog: Option<ui::export_dialog::ExportDialog>,
    pub export_job: Option<ExportJob>,
    /// การตั้งค่าโปรแกรม (จำไว้ใน settings.json) และสำเนาล่าสุดที่บันทึกลงไฟล์แล้ว
    pub settings: crate::settings::Settings,
    settings_saved: crate::settings::Settings,
    pub settings_open: bool,
    /// ตอนเปิดโปรแกรมพบงานที่ยังไม่ได้บันทึกจากครั้งก่อน
    pub recovery_prompt: bool,
    /// เวลาที่เริ่มมีการแก้ที่ยังไม่บันทึก (ใช้นับเวลาบันทึกอัตโนมัติ)
    dirty_since: Option<std::time::Instant>,
    /// บันทึกงานลงไฟล์ครั้งล่าสุด
    pub last_saved: Option<std::time::Instant>,
    /// ซับอัตโนมัติที่กำลังถอดอยู่ รอใส่ผลลงไทม์ไลน์
    pub subtitle_dialog: Option<ui::subtitle_dialog::SubtitleRun>,
    pub transcribe_job: Option<TranscribeJob>,

    /// รับคำสั่งจาก AI ผ่าน MCP
    pub mcp: crate::mcp::McpState,
    /// ตำแหน่งการ์ดบนหน้าจอ (จำไว้ในไฟล์) และสถานะระหว่างลากการ์ด
    pub layout: ui::layout::Layout,
    pub card_drag: ui::layout::DragState,

    toast: Option<(String, f64)>,
}

impl TadTorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::load_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let ffmpeg = FfmpegProbe::start(cc.egui_ctx.clone());
        let settings = crate::settings::load();
        let mut app = Self {
            project: settings.new_project.project(),
            library: MediaLibrary::default(),
            jobs: MediaJobs::start(cc.egui_ctx.clone(), ffmpeg.clone()),
            ffmpeg,
            timeline: Timeline::default(),
            history: History::default(),
            timeline_rev: 0,
            drag_origin: None,
            thumbs: HashMap::new(),
            logo: crate::app_icon().map(|icon| {
                let img = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
                cc.egui_ctx.load_texture("logo", img, egui::TextureOptions::LINEAR)
            }),
            waveforms: HashMap::new(),
            left_tab: LeftTab::Media,
            selected_media: None,
            selected_clip: None,
            playhead: 0.0,
            playing: false,
            px_per_sec: 40.0,
            player: None,
            audio_out: None,
            audio_error: None,
            grabber: FrameGrabber::start(cc.egui_ctx.clone()),
            grab_key: None,
            grab_id: 0,
            preview_tex: None,
            project_path: None,
            dirty: false,
            pending: None,
            allow_close: false,
            export_dialog: None,
            export_job: None,
            settings: settings.clone(),
            settings_saved: settings,
            settings_open: false,
            recovery_prompt: false,
            dirty_since: None,
            last_saved: None,
            subtitle_dialog: None,
            transcribe_job: None,
            mcp: crate::mcp::McpState::start(cc.egui_ctx.clone()),
            layout: ui::layout::load(),
            card_drag: Default::default(),
            toast: None,
        };
        cc.egui_ctx.set_zoom_factor(app.layout.ui_zoom);
        // เปิดไฟล์โปรเจกต์ที่ส่งมาทางบรรทัดคำสั่ง (เช่น ดับเบิลคลิกไฟล์ .tadtor)
        // ไม่งั้น: มีงานค้างจากครั้งก่อนที่ปิดกะทันหัน = ถามกู้คืน, หรือเปิดงานล่าสุด
        let arg = std::env::args_os().nth(1).map(PathBuf::from).filter(|p| p.extension().is_some_and(|e| e == project_file::EXTENSION));
        if let Some(path) = arg {
            app.open_project_path(&cc.egui_ctx, &path);
        } else if crate::settings::recovery_file().is_file() {
            app.recovery_prompt = true;
        } else if let Some(last) = app.settings.recent.first().filter(|p| app.settings.open_last_project && p.is_file()).cloned() {
            app.open_project_path(&cc.egui_ctx, &last);
        }
        app
    }

    /// แสดงข้อความแจ้งเตือนสั้น ๆ มุมล่างของพรีวิว
    pub fn notify(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        let until = ctx.input(|i| i.time) + 2.5;
        self.toast = Some((msg.into(), until));
    }

    // ---------------- คลังสื่อ ----------------

    /// เปิดหน้าต่างเลือกไฟล์ของ Windows แล้วเพิ่มลงคลังสื่อ
    pub fn import_dialog(&mut self, ctx: &egui::Context) {
        let mut all: Vec<&str> = Vec::new();
        all.extend_from_slice(MediaKind::VIDEO_EXT);
        all.extend_from_slice(MediaKind::AUDIO_EXT);
        all.extend_from_slice(MediaKind::IMAGE_EXT);
        let picked = rfd::FileDialog::new()
            .set_title("นำเข้าไฟล์สื่อ")
            .add_filter("ไฟล์สื่อทั้งหมด", &all)
            .add_filter("วิดีโอ", MediaKind::VIDEO_EXT)
            .add_filter("เสียง", MediaKind::AUDIO_EXT)
            .add_filter("รูปภาพ", MediaKind::IMAGE_EXT)
            .pick_files();
        if let Some(paths) = picked {
            self.add_media(ctx, paths);
        }
    }

    pub fn add_media(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        let total = paths.len();
        let added = self.add_media_quiet(paths);
        let skipped = total - added;
        let msg = match (added, skipped) {
            (0, _) => "ไม่มีไฟล์ใหม่ที่รองรับ".to_owned(),
            (a, 0) => format!("นำเข้า {a} ไฟล์แล้ว"),
            (a, s) => format!("นำเข้า {a} ไฟล์ (ข้าม {s} ไฟล์ที่ซ้ำหรือไม่รองรับ)"),
        };
        self.notify(ctx, msg);
    }

    pub fn add_media_quiet(&mut self, paths: Vec<PathBuf>) -> usize {
        let added = self.library.add_paths(paths);
        for path in &added {
            if let Some(item) = self.library.get(path) {
                self.jobs.enqueue(path.clone(), item.kind);
            }
        }
        if !added.is_empty() {
            self.dirty = true;
        }
        added.len()
    }

    pub fn remove_media(&mut self, path: &Path) {
        self.library.items.retain(|it| it.path != path);
        if self.selected_media.as_deref() == Some(path) {
            self.selected_media = None;
        }
        self.dirty = true;
    }

    fn handle_media_events(&mut self, ctx: &egui::Context) {
        for ev in self.jobs.poll() {
            match ev {
                MediaEvent::Probed(path, result) => {
                    if let Some(item) = self.library.get_mut(&path) {
                        item.state = match result {
                            Ok(info) => MediaState::Ready(info),
                            Err(e) => MediaState::Failed(e),
                        };
                    }
                }
                MediaEvent::Thumbnail(path, img) => {
                    let tex = ctx.load_texture(path.display().to_string(), img, egui::TextureOptions::LINEAR);
                    self.thumbs.insert(path, tex);
                }
                MediaEvent::Waveform(path, peaks) => {
                    self.waveforms.insert(path, Arc::new(peaks));
                }
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if dropped.is_empty() {
            return;
        }
        // ลากไฟล์โปรเจกต์มาวาง = เปิดโปรเจกต์
        if let Some(p) = dropped.iter().find(|p| p.extension().is_some_and(|e| e == project_file::EXTENSION)) {
            let p = p.clone();
            self.open_project_path(ctx, &p);
            return;
        }
        self.add_media(ctx, dropped);
    }

    // ---------------- แก้ไทม์ไลน์ ----------------

    /// แก้ไทม์ไลน์หนึ่งครั้ง (เก็บลง Undo ให้อัตโนมัติถ้ามีอะไรเปลี่ยน)
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut Timeline) -> R) -> R {
        let before = self.timeline.clone();
        let r = f(&mut self.timeline);
        if self.timeline != before {
            self.history.push(before);
            self.timeline_changed();
        }
        r
    }

    /// เรียกหลังไทม์ไลน์เปลี่ยน (หยุดเล่น, เรนเดอร์ภาพใหม่, ตรวจสิ่งที่เลือกไว้)
    pub fn timeline_changed(&mut self) {
        self.timeline_rev += 1;
        self.dirty = true;
        self.stop_playback();
        if let Some(id) = self.selected_clip {
            if self.timeline.clip(id).is_none() {
                self.selected_clip = None;
            }
        }
        self.playhead = self.playhead.min(self.timeline_end());
    }

    pub fn begin_drag(&mut self) {
        self.drag_origin = Some(self.timeline.clone());
        self.stop_playback();
    }

    pub fn end_drag(&mut self) {
        if let Some(origin) = self.drag_origin.take() {
            if origin != self.timeline {
                self.history.push(origin);
                self.timeline_changed();
            }
        }
    }

    /// ใส่ไฟล์จากคลังสื่อลงไทม์ไลน์ (`main_index` = ลำดับในแทร็กหลัก, `audio_at` = เวลาในแทร็กเสียง)
    pub fn add_to_timeline(&mut self, ctx: &egui::Context, path: &Path, main_index: Option<usize>, audio_at: Option<f64>) {
        let Some(item) = self.library.get(path) else { return };
        let info = match &item.state {
            MediaState::Ready(info) => info.clone(),
            MediaState::Probing => {
                self.notify(ctx, "กำลังอ่านไฟล์ รอสักครู่แล้วลองใหม่");
                return;
            }
            MediaState::Failed(e) => {
                self.notify(ctx, format!("ใช้ไฟล์นี้ไม่ได้: {e}"));
                return;
            }
        };
        let (kind, name, path) = (item.kind, item.name.clone(), item.path.clone());
        let playhead = self.playhead;
        let id = self.edit(|tl| {
            let clip = tl.new_clip(path, name, kind, &info);
            if kind.is_visual() {
                let at = main_index.unwrap_or(tl.main.len());
                tl.insert_main(at, clip)
            } else {
                tl.place_audio(clip, audio_at.unwrap_or(playhead))
            }
        });
        self.selected_clip = Some(id);
        self.selected_media = None;
    }

    /// คลิปที่คำสั่งตัด/แยกจะทำงานด้วย: คลิปที่เลือก (ถ้าหัวเล่นอยู่บนคลิปนั้น) ไม่งั้นคลิปแทร็กหลักใต้หัวเล่น
    fn target_clip(&self) -> Option<Clip> {
        let t = self.playhead;
        if let Some(c) = self.selected_clip.and_then(|id| self.timeline.clip(id)) {
            if c.contains(t) {
                return Some(c.clone());
            }
        }
        self.timeline.clip_at(TrackKind::Main, t).filter(|c| c.contains(t)).cloned()
    }

    pub fn split_at_playhead(&mut self, ctx: &egui::Context) {
        let Some(c) = self.target_clip() else {
            self.notify(ctx, "วางหัวเล่นบนคลิปที่จะแยกก่อน");
            return;
        };
        let t = self.playhead;
        if let Some(right) = self.edit(|tl| tl.split(c.id, t)) {
            self.selected_clip = Some(right);
        }
    }

    pub fn trim_head_to_playhead(&mut self, ctx: &egui::Context) {
        let Some(c) = self.target_clip() else {
            self.notify(ctx, "วางหัวเล่นบนคลิปที่จะตัดหัวก่อน");
            return;
        };
        let t = self.playhead;
        self.edit(|tl| tl.trim_head(c.id, t - c.start));
        if self.timeline.find(c.id).is_some_and(|(k, _)| k == TrackKind::Main) {
            self.playhead = c.start;
        }
        self.selected_clip = Some(c.id);
    }

    pub fn trim_tail_to_playhead(&mut self, ctx: &egui::Context) {
        let Some(c) = self.target_clip() else {
            self.notify(ctx, "วางหัวเล่นบนคลิปที่จะตัดท้ายก่อน");
            return;
        };
        let t = self.playhead;
        self.edit(|tl| tl.trim_tail(c.id, t - c.end()));
        self.selected_clip = Some(c.id);
    }

    /// เพิ่มข้อความที่หัวเล่น
    pub fn add_text(&mut self, style: TextStyle) {
        let at = self.playhead;
        let id = self.edit(|tl| {
            let c = tl.new_text_clip(style, TEXT_DEFAULT);
            tl.place(TrackKind::Text, c, at)
        });
        self.selected_clip = Some(id);
        self.selected_media = None;
    }

    /// คลิปที่เลือกอยู่ ถ้าตรงเงื่อนไข ไม่งั้นแจ้งผู้ใช้ว่าต้องเลือกอะไร
    pub fn selected_where(&mut self, ctx: &egui::Context, want: Want) -> Option<Clip> {
        let c = self.selected_clip.and_then(|id| self.timeline.clip(id)).cloned();
        let track = c.as_ref().and_then(|c| self.timeline.find(c.id)).map(|f| f.0);
        let ok = match (&c, want) {
            (None, _) => false,
            (Some(_), Want::Visual) => track == Some(TrackKind::Main),
            (Some(c), Want::Sound) => c.has_audio,
            (Some(_), Want::Text) => track.is_some_and(TrackKind::is_text),
        };
        if ok {
            return c;
        }
        let msg = match want {
            Want::Visual => "เลือกคลิปวิดีโอ/รูปในแทร็กหลักก่อน",
            Want::Sound => "เลือกคลิปที่มีเสียงก่อน",
            Want::Text => "เลือกคลิปข้อความก่อน",
        };
        self.notify(ctx, msg);
        None
    }

    /// แก้ค่าด้วยตัวเลื่อน/ช่องพิมพ์: ระหว่างลากแก้สด ๆ แล้วเก็บ Undo ครั้งเดียว
    pub fn live_edit(&mut self, resp: &egui::Response, f: impl FnOnce(&mut Timeline)) {
        if resp.drag_started() || resp.gained_focus() {
            self.begin_drag();
        }
        if resp.changed() {
            if self.drag_origin.is_some() {
                f(&mut self.timeline);
                self.timeline.pack();
                self.timeline_rev += 1;
                self.dirty = true;
            } else {
                self.edit(f);
            }
        }
        if resp.drag_stopped() || resp.lost_focus() {
            self.end_drag();
        }
    }

    pub fn import_srt(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new().set_title("นำเข้าซับไตเติล").add_filter("ซับไตเติล SRT", &["srt"]).pick_file()
        else {
            return;
        };
        let text = match std::fs::read(&path) {
            Ok(b) => String::from_utf8(b.clone()).unwrap_or_else(|_| String::from_utf8_lossy(&b).into_owned()),
            Err(e) => {
                self.notify(ctx, format!("เปิดไฟล์ไม่ได้: {e}"));
                return;
            }
        };
        let items = subtitles::parse_srt(&text);
        if items.is_empty() {
            self.notify(ctx, "ไม่พบซับไตเติลในไฟล์นี้");
            return;
        }
        let n = items.len();
        self.edit(|tl| tl.replace_subtitles(items));
        self.notify(ctx, format!("นำเข้าซับไตเติล {n} บรรทัด"));
    }

    pub fn export_srt(&mut self, ctx: &egui::Context) {
        let items: Vec<(f64, f64, String)> =
            self.timeline.subtitles.iter().filter_map(|c| Some((c.start, c.end(), c.text.as_ref()?.content.clone()))).collect();
        if items.is_empty() {
            self.notify(ctx, "ยังไม่มีซับไตเติล");
            return;
        }
        let Some(mut path) = rfd::FileDialog::new()
            .set_title("ส่งออกซับไตเติล")
            .set_file_name(format!("{}.srt", self.project.name))
            .add_filter("ซับไตเติล SRT", &["srt"])
            .save_file()
        else {
            return;
        };
        if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("srt")) {
            path.set_extension("srt");
        }
        match std::fs::write(&path, subtitles::write_srt(&items)) {
            Ok(()) => self.notify(ctx, "ส่งออกซับไตเติลแล้ว"),
            Err(e) => self.notify(ctx, format!("บันทึกไม่ได้: {e}")),
        }
    }

    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selected_clip.take() {
            self.edit(|tl| tl.remove(id));
        }
    }

    pub fn undo(&mut self) {
        self.drag_origin = None;
        if self.history.undo(&mut self.timeline) {
            self.timeline_changed();
        }
    }

    pub fn redo(&mut self) {
        self.drag_origin = None;
        if self.history.redo(&mut self.timeline) {
            self.timeline_changed();
        }
    }

    // ---------------- เล่น / พรีวิว ----------------

    /// ความยาวที่ไทม์ไลน์ยอมให้หัวเล่นไปถึง (ตอนยังไม่มีคลิป ให้ลองเลื่อนได้ 60 วินาที)
    pub fn timeline_end(&self) -> f64 {
        let d = self.timeline.duration();
        if d > 0.0 {
            d
        } else {
            60.0
        }
    }

    pub fn preview_spec(&self) -> VideoSpec {
        let (width, height) = self.project.aspect.resolution(self.settings.preview.short_side());
        VideoSpec { width, height, fps: self.project.fps.min(30) }
    }

    pub fn seek(&mut self, t: f64) {
        self.playhead = t.clamp(0.0, self.timeline_end());
        if self.playing {
            self.start_player();
        }
    }

    pub fn toggle_play(&mut self, ctx: &egui::Context) {
        if self.playing {
            self.stop_playback();
            return;
        }
        if self.timeline.is_empty() {
            self.notify(ctx, "ลากสื่อลงไทม์ไลน์ก่อนแล้วค่อยกดเล่น");
            return;
        }
        if self.playhead >= self.timeline.duration() - 1e-3 {
            self.playhead = 0.0;
        }
        self.playing = true;
        self.start_player();
    }

    fn start_player(&mut self) {
        let Some(info) = self.ffmpeg.info() else {
            self.playing = false;
            return;
        };
        self.stop_player();
        if self.audio_out.is_none() && self.audio_error.is_none() {
            match AudioOut::open() {
                Ok(out) => self.audio_out = Some(out),
                Err(e) => self.audio_error = Some(e),
            }
        }
        let spec = self.preview_spec();
        self.player = Some(Player::start(&info.ffmpeg, &self.timeline, self.playhead, spec, self.audio_out.as_ref()));
    }

    fn stop_player(&mut self) {
        if let Some(out) = &self.audio_out {
            out.set_source(None);
        }
        self.player = None;
    }

    pub fn stop_playback(&mut self) {
        self.stop_player();
        self.playing = false;
    }

    fn tick_playback(&mut self, ctx: &egui::Context) {
        let Some(player) = &mut self.player else {
            return;
        };
        let up = player.update();
        let (until, error) = (player.until, player.error.lock().unwrap().clone());
        self.playhead = up.time;
        if let Some(img) = up.frame {
            self.set_preview_image(ctx, img);
        }
        if up.finished {
            if until < self.timeline.duration() - 1e-3 {
                // ยังไม่จบงาน แค่จบช่วงพรีวิว เล่นช่วงถัดไปต่อเลย
                self.playhead = until;
                self.start_player();
            } else {
                self.stop_playback();
                self.playhead = self.timeline.duration();
                if let Some(e) = error {
                    self.notify(ctx, format!("พรีวิวมีปัญหา: {e}"));
                }
            }
        }
        ctx.request_repaint();
    }

    fn set_preview_image(&mut self, ctx: &egui::Context, img: egui::ColorImage) {
        match &mut self.preview_tex {
            Some(tex) if tex.size() == img.size => tex.set(img, egui::TextureOptions::LINEAR),
            _ => self.preview_tex = Some(ctx.load_texture("preview", img, egui::TextureOptions::LINEAR)),
        }
    }

    /// ตอนไม่ได้เล่น: ขอภาพนิ่งที่หัวเล่นมาแสดง (ถ้าตำแหน่ง/ไทม์ไลน์เปลี่ยน)
    fn update_still_frame(&mut self, ctx: &egui::Context) {
        if let Some((id, img)) = self.grabber.take() {
            if id == self.grab_id {
                self.set_preview_image(ctx, img);
            }
        }
        if self.playing {
            return;
        }
        if self.timeline.main.is_empty() {
            self.preview_tex = None;
            self.grab_key = None;
            return;
        }
        let spec = self.preview_spec();
        let frame = (self.playhead * spec.fps as f64).floor() as i64;
        let key = (self.timeline_rev, frame, spec);
        if self.grab_key == Some(key) {
            return;
        }
        let Some(info) = self.ffmpeg.info() else { return };
        self.grab_key = Some(key);
        self.grab_id += 1;
        let t = frame as f64 / spec.fps as f64;
        self.grabber.request(self.grab_id, info.ffmpeg, self.timeline.clone(), t, spec);
    }

    // ---------------- โปรเจกต์ ----------------

    /// ทำงานที่ขอไว้ (ถ้างานยังไม่บันทึก ให้ถามก่อน)
    pub fn request(&mut self, ctx: &egui::Context, action: PendingAction) {
        if self.dirty && !self.timeline.is_empty() {
            self.pending = Some(action);
        } else {
            self.perform(ctx, action);
        }
    }

    pub fn perform(&mut self, ctx: &egui::Context, action: PendingAction) {
        match action {
            PendingAction::Close => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            PendingAction::NewProject => self.reset_project(),
            PendingAction::OpenPath(p) => self.open_project_path(ctx, &p),
            PendingAction::OpenProject => {
                let picked =
                    rfd::FileDialog::new().set_title("เปิดโปรเจกต์").add_filter("โปรเจกต์ TadTor", &[project_file::EXTENSION]).pick_file();
                if let Some(p) = picked {
                    self.open_project_path(ctx, &p);
                }
            }
        }
    }

    fn reset_project(&mut self) {
        self.stop_playback();
        self.project = self.settings.new_project.project();
        // ผู้ใช้เลือกแล้วว่าจะบันทึกหรือทิ้งงานเดิม ไฟล์สำรองไม่ต้องใช้แล้ว
        let _ = std::fs::remove_file(crate::settings::recovery_file());
        self.last_saved = None;
        self.library = MediaLibrary::default();
        self.timeline = Timeline::default();
        self.history.clear();
        self.timeline_rev += 1;
        self.selected_clip = None;
        self.selected_media = None;
        self.playhead = 0.0;
        self.project_path = None;
        self.dirty = false;
    }

    pub fn open_project_path(&mut self, ctx: &egui::Context, path: &Path) {
        match project_file::load(path) {
            Ok(file) => {
                self.reset_project();
                self.project = file.project;
                self.timeline = file.timeline;
                // ไฟล์ที่คลิปใช้อยู่แต่ไม่ได้อยู่ในคลังสื่อ ให้ใส่คลังด้วย (จะได้มีภาพย่อ)
                let mut media = file.media;
                for c in self.timeline.main.iter().chain(self.timeline.audio.iter()) {
                    if !media.contains(&c.path) {
                        media.push(c.path.clone());
                    }
                }
                self.add_media_quiet(media);
                self.project_path = Some(path.to_path_buf());
                self.dirty = false;
                self.settings.remember(path);
                let missing = self.timeline.main.iter().chain(self.timeline.audio.iter()).filter(|c| !c.path.exists()).count();
                if missing > 0 {
                    self.notify(ctx, format!("เปิดโปรเจกต์แล้ว แต่หาไฟล์ไม่เจอ {missing} คลิป (ถูกย้ายหรือลบ)"));
                } else {
                    self.notify(ctx, format!("เปิด {} แล้ว", self.project.name));
                }
            }
            Err(e) => self.notify(ctx, format!("เปิดโปรเจกต์ไม่ได้: {e}")),
        }
    }

    /// กู้คืนงานจากไฟล์สำรอง (เป็นงานที่ยังไม่เคยบันทึก จึงไม่มีที่อยู่ไฟล์)
    pub fn restore_recovery(&mut self, ctx: &egui::Context) {
        let file = crate::settings::recovery_file();
        let recent = self.settings.recent.clone();
        self.open_project_path(ctx, &file);
        // ไฟล์สำรองไม่ใช่งานของผู้ใช้ ไม่ต้องจำไว้ในงานล่าสุด และต้องถามที่บันทึกตอนกด Ctrl+S
        self.settings.recent = recent;
        self.project_path = None;
        self.dirty = true;
        self.notify(ctx, "กู้คืนงานแล้ว อย่าลืมบันทึก (Ctrl+S)");
    }

    /// บันทึกอัตโนมัติ: งานที่มีไฟล์แล้วบันทึกทับ งานที่ยังไม่เคยบันทึกเก็บเป็นไฟล์สำรอง
    fn autosave(&mut self, ctx: &egui::Context) {
        if !self.dirty || !self.settings.autosave {
            self.dirty_since = None;
            return;
        }
        let since = *self.dirty_since.get_or_insert_with(std::time::Instant::now);
        let every = std::time::Duration::from_secs(self.settings.autosave_minutes.clamp(1, 60) as u64 * 60);
        let waited = since.elapsed();
        // ไม่บันทึกระหว่างลากแก้ค่า หรือมีหน้าต่างถามค้างอยู่
        if waited < every || self.drag_origin.is_some() || self.pending.is_some() {
            ctx.request_repaint_after(every.saturating_sub(waited).max(std::time::Duration::from_secs(1)));
            return;
        }
        let media = self.library.items.iter().map(|it| it.path.clone()).collect();
        match self.project_path.clone() {
            Some(path) => {
                if project_file::save(&path, &self.project, media, &self.timeline).is_ok() {
                    self.dirty = false;
                    self.last_saved = Some(std::time::Instant::now());
                }
            }
            None => {
                let _ = std::fs::create_dir_all(crate::settings::dir());
                let _ = project_file::save(&crate::settings::recovery_file(), &self.project, media, &self.timeline);
            }
        }
        self.dirty_since = None;
    }

    /// บันทึก คืน true ถ้าบันทึกสำเร็จ (`save_as` = ถามที่เก็บไฟล์ใหม่เสมอ)
    pub fn save(&mut self, ctx: &egui::Context, save_as: bool) -> bool {
        let path = match (&self.project_path, save_as) {
            (Some(p), false) => p.clone(),
            _ => {
                let picked = rfd::FileDialog::new()
                    .set_title("บันทึกโปรเจกต์")
                    .set_file_name(format!("{}.{}", self.project.name, project_file::EXTENSION))
                    .add_filter("โปรเจกต์ TadTor", &[project_file::EXTENSION])
                    .save_file();
                let Some(mut p) = picked else { return false };
                if p.extension().is_none_or(|e| e != project_file::EXTENSION) {
                    p.set_extension(project_file::EXTENSION);
                }
                p
            }
        };
        let media = self.library.items.iter().map(|it| it.path.clone()).collect();
        match project_file::save(&path, &self.project, media, &self.timeline) {
            Ok(()) => {
                self.settings.remember(&path);
                self.project_path = Some(path);
                self.dirty = false;
                self.last_saved = Some(std::time::Instant::now());
                let _ = std::fs::remove_file(crate::settings::recovery_file());
                self.notify(ctx, "บันทึกแล้ว");
                true
            }
            Err(e) => {
                self.notify(ctx, format!("บันทึกไม่สำเร็จ: {e}"));
                false
            }
        }
    }

    pub fn open_export_dialog(&mut self, ctx: &egui::Context) {
        if self.timeline.is_empty() {
            self.notify(ctx, "ยังไม่มีคลิปในไทม์ไลน์ให้ส่งออก");
            return;
        }
        if self.export_job.is_none() {
            self.stop_playback();
            self.export_dialog = Some(ui::export_dialog::ExportDialog::new(self));
        }
    }

    // ---------------- คีย์ลัด ----------------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // ไม่รับคีย์ลัดตอนกำลังพิมพ์ในช่องข้อความ หรือมีหน้าต่างถามค้างอยู่
        if ctx.egui_wants_keyboard_input()
            || self.pending.is_some()
            || self.settings_open
            || self.recovery_prompt
            || self.export_dialog.is_some()
            || self.subtitle_dialog.is_some()
            || self.transcribe_job.is_some()
        {
            return;
        }
        use egui::{Key, Modifiers};
        let cmd = Modifiers::COMMAND;
        let shift_cmd = Modifiers::COMMAND | Modifiers::SHIFT;
        let k = |m: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(m, key));

        if k(shift_cmd, Key::Z) || k(cmd, Key::Y) {
            self.redo();
        }
        if k(cmd, Key::Z) {
            self.undo();
        }
        if k(shift_cmd, Key::S) {
            self.save(ctx, true);
        }
        if k(cmd, Key::S) {
            self.save(ctx, false);
        }
        if k(cmd, Key::O) {
            self.request(ctx, PendingAction::OpenProject);
        }
        if k(cmd, Key::N) {
            self.request(ctx, PendingAction::NewProject);
        }
        if k(cmd, Key::E) {
            self.open_export_dialog(ctx);
        }
        if k(cmd, Key::I) {
            self.import_dialog(ctx);
        }
        if k(cmd, Key::Comma) {
            self.settings_open = true;
        }
        if k(Modifiers::NONE, Key::T) {
            self.add_text(TextStyle::default());
        }
        if k(cmd, Key::B) {
            self.split_at_playhead(ctx);
        }
        let none = Modifiers::NONE;
        if k(none, Key::Q) {
            self.trim_head_to_playhead(ctx);
        }
        if k(none, Key::W) {
            self.trim_tail_to_playhead(ctx);
        }
        if k(none, Key::Delete) || k(none, Key::Backspace) {
            self.delete_selected();
        }
        if k(none, Key::Escape) {
            self.selected_clip = None;
            self.selected_media = None;
        }
        if k(none, Key::Space) {
            self.toggle_play(ctx);
        }
        let frame = 1.0 / self.project.fps as f64;
        if k(none, Key::Home) {
            self.seek(0.0);
        }
        if k(none, Key::End) {
            self.seek(self.timeline_end());
        }
        if k(none, Key::ArrowLeft) {
            self.seek(self.playhead - frame);
        }
        if k(none, Key::ArrowRight) {
            self.seek(self.playhead + frame);
        }
        if k(Modifiers::SHIFT, Key::ArrowLeft) {
            self.seek(self.playhead - 1.0);
        }
        if k(Modifiers::SHIFT, Key::ArrowRight) {
            self.seek(self.playhead + 1.0);
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        let must_ask = self.export_job.is_some() || (self.dirty && !self.timeline.is_empty());
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && must_ask {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.export_job.is_some() {
                self.notify(ctx, "กำลังส่งออกอยู่ ยกเลิกการส่งออกก่อนแล้วค่อยปิด");
            } else {
                self.pending = Some(PendingAction::Close);
            }
        }
    }

    fn show_toast(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let Some((msg, until)) = &self.toast else {
            return;
        };
        if now > *until {
            self.toast = None;
            return;
        }
        let msg = msg.clone();
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 56.0))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(theme::SURFACE_HOVER)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.label(msg);
                    });
            });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
}

impl eframe::App for TadTorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close_request(&ctx);
        self.handle_media_events(&ctx);
        self.handle_dropped_files(&ctx);
        self.handle_shortcuts(&ctx);
        self.tick_playback(&ctx);
        self.update_still_frame(&ctx);

        // ลำดับสำคัญ: แผงบนสุดก่อน แล้วการ์ดต่าง ๆ ตามที่ผู้ใช้จัดไว้
        ui::top_bar::show(self, ui);
        ui::layout::show(self, ui);

        ui::dialogs::show(self, &ctx);
        ui::export_dialog::show(self, &ctx);
        ui::subtitle_dialog::show(self, &ctx);
        ui::settings_dialog::show(self, &ctx);
        crate::mcp::poll(self, &ctx);
        self.autosave(&ctx);
        if self.settings != self.settings_saved {
            crate::settings::save(&self.settings);
            self.settings_saved = self.settings.clone();
        }
        self.show_toast(ui);
    }

    fn on_exit(&mut self) {
        crate::mcp::control::remove_info();
        // ปิดปกติ = ผู้ใช้บันทึกหรือเลือกทิ้งงานแล้ว (ปิดกะทันหันจะไม่มาถึงตรงนี้ ไฟล์สำรองจึงยังอยู่)
        let _ = std::fs::remove_file(crate::settings::recovery_file());
        crate::settings::save(&self.settings);
        self.stop_playback();
        if let Some(job) = &self.transcribe_job {
            job.cancel();
        }
        if let Some(job) = &self.export_job {
            job.cancel();
        }
    }
}
