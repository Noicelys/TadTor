//! งานที่ทำเบื้องหลัง: อ่านไฟล์สื่อ, เล่นพรีวิว, เสียง, ส่งออก

pub mod audio;
pub mod export;
pub mod jobs;
pub mod player;
pub mod transcribe;

/// ทดสอบกับ FFmpeg และไฟล์จริง: `TADTOR_MEDIA=<โฟลเดอร์ไฟล์ทดสอบ> cargo test -- --ignored`
#[cfg(test)]
mod integration {
    use std::path::{Path, PathBuf};
    use std::process::Stdio;

    use crate::core::media::{MediaInfo, MediaKind};
    use crate::core::timeline::Timeline;
    use crate::engine::export::{Encoder, ExportJob, ExportSettings, ExportState, Quality};
    use crate::ffmpeg::graph::{self, VideoSpec};
    use crate::ffmpeg::{command, probe};

    fn tools() -> (PathBuf, PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ffmpeg").join("bin");
        let media = PathBuf::from(std::env::var("TADTOR_MEDIA").expect("ตั้ง TADTOR_MEDIA ก่อน"));
        (root.join("ffmpeg.exe"), root.join("ffprobe.exe"), media)
    }

    fn info(ffprobe: &Path, p: &Path) -> (MediaKind, MediaInfo) {
        let kind = MediaKind::from_path(p).unwrap();
        (kind, probe::probe(ffprobe, p, kind).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
    }

    fn add(tl: &mut Timeline, ffprobe: &Path, p: &Path) -> u64 {
        let (kind, inf) = info(ffprobe, p);
        let c = tl.new_clip(p.to_path_buf(), p.file_name().unwrap().to_string_lossy().into_owned(), kind, &inf);
        if kind.is_visual() {
            let n = tl.main.len();
            tl.insert_main(n, c)
        } else {
            tl.place_audio(c, 1.0)
        }
    }

    fn sample_timeline(ffprobe: &Path, media: &Path) -> Timeline {
        let mut tl = Timeline::default();
        let a = add(&mut tl, ffprobe, &media.join("คลิป ทดสอบ.mp4"));
        let b = tl.split(a, 2.0).unwrap();
        tl.trim_head(b, 1.0); // คลิปหลังเหลือวินาทีที่ 3-6 ของไฟล์
        add(&mut tl, ffprobe, &media.join("rotated.mp4"));
        add(&mut tl, ffprobe, &media.join("tall_noaudio.mp4"));
        let pic = add(&mut tl, ffprobe, &media.join("pic.png"));
        tl.set_duration(pic, 2.0);
        let song = add(&mut tl, ffprobe, &media.join("song.mp3"));
        tl.trim_tail(song, -2.0);
        // ยาว: 2 + 3 + 3 + 4 + 2 = 14 วิ (เพลง 1..9)
        tl
    }

    #[test]
    #[ignore]
    fn probe_real_files() {
        let (ffmpeg, ffprobe, media) = tools();
        let (_, rot) = info(&ffprobe, &media.join("rotated.mp4"));
        assert_eq!((rot.width, rot.height), (720, 1280), "วิดีโอหมุน 90° ต้องสลับกว้าง/สูง");
        let (_, tall) = info(&ffprobe, &media.join("tall_noaudio.mp4"));
        assert!(!tall.has_audio && tall.has_video);
        let (_, song) = info(&ffprobe, &media.join("song.mp3"));
        assert!((song.duration.unwrap() - 10.0).abs() < 0.1);
        let (k, pic) = info(&ffprobe, &media.join("pic.png"));
        assert_eq!((k, pic.duration, pic.width), (MediaKind::Image, None, 800));

        let (w, h, px) = probe::thumbnail(&ffmpeg, &media.join("rotated.mp4"), MediaKind::Video, &rot).unwrap();
        assert_eq!((w, h, px.len()), (60, 108, 60 * 108 * 4));
        let wave = probe::waveform(&ffmpeg, &media.join("song.mp3")).unwrap();
        assert!((wave.len() as i64 - 1000).abs() < 30, "{}", wave.len());
        // เสียง sine ของ lavfi ดังแค่ 1/8 โดยปริยาย
        let peak = wave.iter().copied().fold(0.0, f32::max);
        assert!((peak - 0.125).abs() < 0.02, "{peak}");
    }

    /// รัน graph พรีวิวจริงแล้วนับจำนวนเฟรม/ตัวอย่างเสียงที่ได้
    #[test]
    #[ignore]
    fn preview_streams_have_right_length() {
        let (ffmpeg, ffprobe, media) = tools();
        let tl = sample_timeline(&ffprobe, &media);
        assert!((tl.duration() - 14.0).abs() < 1e-6, "{}", tl.duration());
        let spec = VideoSpec { width: 320, height: 180, fps: 30 };
        let from = 1.0;
        let g = graph::build(&tl, from, tl.duration(), Some(spec), None);
        let out = command(&ffmpeg)
            .current_dir(&g.cwd)
            .args(["-v", "error"])
            .args(&g.inputs)
            .arg("-filter_complex")
            .arg(&g.filter)
            .args(["-map", "[vout]", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stderr), g.filter);
        let frames = out.stdout.len() / (320 * 180 * 4);
        let want = ((tl.duration() - from) * 30.0) as i64;
        assert!((frames as i64 - want).abs() <= 6, "ได้ {frames} เฟรม ควรได้ราว {want}");

        let g = graph::build(&tl, from, tl.duration(), None, Some(48_000));
        let out = command(&ffmpeg)
            .current_dir(&g.cwd)
            .args(["-v", "error"])
            .args(&g.inputs)
            .arg("-filter_complex")
            .arg(&g.filter)
            .args(["-map", "[aout]", "-f", "f32le", "-ac", "2", "-ar", "48000", "pipe:1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stderr), g.filter);
        let secs = out.stdout.len() as f64 / (48_000.0 * 2.0 * 4.0);
        assert!((secs - (tl.duration() - from)).abs() < 0.05, "เสียงยาว {secs}");
    }

    fn run_graph(ffmpeg: &Path, g: &graph::Graph, out: &[&str]) -> std::process::Output {
        command(ffmpeg)
            .current_dir(&g.cwd)
            .args(["-v", "error"])
            .args(&g.inputs)
            .arg("-filter_complex")
            .arg(&g.filter)
            .args(out)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap()
    }

    /// ไทม์ไลน์ที่ใช้ทุกอย่างของเฟส 3-5: ข้อความไทย ซับ ฟิลเตอร์ เอฟเฟกต์ ทรานซิชัน เสียง
    fn fancy_timeline(ffprobe: &Path, media: &Path) -> Timeline {
        use crate::core::fx::*;
        use crate::core::timeline::TrackKind;
        let mut tl = sample_timeline(ffprobe, media);
        let ids: Vec<u64> = tl.main.iter().map(|c| c.id).collect();
        tl.update_clip(ids[0], |c| {
            c.fx.transition = Some(Transition { kind: TransitionKind::Dissolve, dur: 0.5 });
            c.fx.filter = FilterPreset::Warm;
            c.fx.filter_amount = 0.5;
            c.fx.fade_in = 0.5;
        });
        tl.update_clip(ids[1], |c| {
            c.fx.transition = Some(Transition { kind: TransitionKind::CircleOpen, dur: 0.8 });
            c.fx.brightness = 0.3;
            c.fx.contrast = 0.2;
            c.fx.saturation = -0.5;
            c.fx.temperature = 0.6;
            c.fx.sharpen = 0.5;
            c.fx.vignette = 0.5;
            c.fx.effect = Effect::Glow;
            c.fx.voice = Voice::Chipmunk;
            c.fx.volume = 1.5;
        });
        tl.update_clip(ids[2], |c| c.fx.effect = Effect::Cinema);
        let t = tl.new_text_clip(
            TextStyle {
                content: "สวัสดีครับ ทดสอบภาษาไทย
บรรทัดที่สอง"
                    .into(),
                ..Default::default()
            },
            3.0,
        );
        tl.place(TrackKind::Text, t, 0.5);
        tl.replace_subtitles(vec![(4.0, 6.0, "ซับไตเติลที่ไม่ใช่คำแรก ญี่ปุ่น ฎีกา ปั้น".into())]);
        let song = tl.audio[0].id;
        tl.update_clip(song, |c| {
            c.fx.fade_in = 1.0;
            c.fx.fade_out = 2.0;
            c.fx.voice = Voice::Echo;
        });
        tl
    }

    /// ภาพเคลื่อนไหวและการแต่งข้อความทุกแบบต้องผ่าน FFmpeg ได้ (ใช้แค่ FFmpeg ในโปรเจกต์ ไม่ต้องมีไฟล์ทดสอบ)
    /// ภาพตัวอย่างทุกแบบเก็บไว้ที่ %TEMP%\tadtor-work\motion-*.png
    #[test]
    #[ignore]
    fn every_text_motion_runs() {
        use crate::core::fx::*;
        use crate::core::timeline::TrackKind;
        let ffmpeg = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ffmpeg").join("bin").join("ffmpeg.exe");
        let spec = VideoSpec { width: 320, height: 180, fps: 10 };
        let base = TextStyle { content: "ทดสอบ Aa".into(), shadow: true, background: true, ..Default::default() };
        let mut cases: Vec<(String, TextStyle)> = Vec::new();
        for &k in TextMotion::ALL {
            cases
                .push((format!("in-{k:?}"), TextStyle { motion: Motion { anim_in: k, in_dur: 1.0, ..Motion::default() }, ..base.clone() }));
            cases.push((
                format!("out-{k:?}"),
                TextStyle { motion: Motion { anim_out: k, out_dur: 1.0, ..Motion::default() }, ..base.clone() },
            ));
        }
        for &k in TextLoop::ALL {
            cases.push((format!("loop-{k:?}"), TextStyle { motion: Motion { looping: k, ..Motion::default() }, ..base.clone() }));
        }
        for &e in Easing::ALL {
            let motion = Motion { anim_in: TextMotion::Up, anim_out: TextMotion::Grow, easing: e, strength: 2.0, ..Motion::default() };
            cases.push((format!("easing-{e:?}"), TextStyle { motion, ..base.clone() }));
        }
        let motion = Motion { looping: TextLoop::Blink, loop_amount: 2.5, ..Motion::default() };
        cases.push(("loop-amount".into(), TextStyle { motion, shadow_angle: 200.0, ..base.clone() }));
        for &a in TextAlign::ALL {
            cases.push((format!("align-{a:?}"), TextStyle { align: a, content: "บรรทัดแรก\nสอง".into(), opacity: 0.6, ..base.clone() }));
        }
        let font = crate::fonts::families().iter().find(|f| f.thai && f.has_italic()).map(|f| f.name.clone()).unwrap_or_default();
        cases.push(("font".into(), TextStyle { font, italic: true, bold: false, ..base.clone() }));
        for (name, style) in cases {
            let mut tl = Timeline::default();
            let c = tl.new_text_clip(style, 2.0);
            tl.place(TrackKind::Text, c, 0.0);
            let mut g = graph::build(&tl, 0.0, 2.0, Some(spec), None);
            g.filter.push_str(";\n[vout]tile=5x4[sheet]");
            let png = graph::work_dir().join(format!("motion-{name}.png"));
            let out = run_graph(&ffmpeg, &g, &["-map", "[sheet]", "-frames:v", "1", "-y", png.to_str().unwrap()]);
            assert!(out.status.success(), "{name}\n{}\n{}", String::from_utf8_lossy(&out.stderr), g.filter);
        }
    }

    /// ฟิลเตอร์ เอฟเฟกต์ ทรานซิชัน เสียง ทุกแบบต้องผ่าน FFmpeg ได้
    #[test]
    #[ignore]
    fn every_fx_runs() {
        use crate::core::fx::*;
        let (ffmpeg, ffprobe, media) = tools();
        let base = sample_timeline(&ffprobe, &media);
        let spec = VideoSpec { width: 160, height: 90, fps: 15 };
        let first = base.main[0].id;
        let mut cases: Vec<(String, Timeline)> = Vec::new();
        for &f in FilterPreset::ALL {
            for amount in [1.0, 0.4] {
                let mut tl = base.clone();
                tl.update_clip(first, |c| {
                    c.fx.filter = f;
                    c.fx.filter_amount = amount;
                });
                cases.push((format!("filter {f:?} {amount}"), tl));
            }
        }
        for &e in Effect::ALL {
            for amount in [1.0, 0.3, 2.0] {
                let mut tl = base.clone();
                tl.update_clip(first, |c| {
                    c.fx.effect = e;
                    c.fx.effect_amount = amount;
                });
                cases.push((format!("effect {e:?} {amount}"), tl));
            }
        }
        // จัดวางภาพ: ทุกแบบวาง x ซูมเล็ก/ใหญ่ x หมุน/กลับด้าน (ทั้งคลิปแนวนอนและแนวตั้ง)
        for &fit in Fit::ALL {
            for (zoom, rotate) in [(0.5, 0u8), (1.0, 1), (2.5, 2), (1.3, 3)] {
                let mut tl = base.clone();
                for id in [first, base.main[2].id] {
                    tl.update_clip(id, |c| {
                        c.fx.fit = fit;
                        c.fx.zoom = zoom;
                        c.fx.rotate = rotate;
                        c.fx.pan_x = 0.7;
                        c.fx.pan_y = -1.0;
                        c.fx.flip_v = rotate == 3;
                        c.fx.background = [30, 200, 90, 255];
                    });
                }
                cases.push((format!("layout {fit:?} {zoom} {rotate}"), tl));
            }
        }
        {
            let mut tl = base.clone();
            tl.update_clip(first, |c| {
                c.fx.hue = -0.4;
                c.fx.tint = 0.6;
                c.fx.highlights = -1.0;
                c.fx.shadows = 1.0;
                c.fx.fade = 0.8;
                c.fx.grain = 1.0;
            });
            cases.push(("adjust extras".into(), tl));
        }
        for &k in TransitionKind::ALL {
            let mut tl = base.clone();
            tl.update_clip(first, |c| c.fx.transition = Some(Transition { kind: k, dur: 0.6 }));
            cases.push((format!("transition {k:?}"), tl));
        }
        for (name, tl) in &cases {
            let g = graph::build(tl, 1.0, 3.0, Some(spec), None);
            let out = run_graph(&ffmpeg, &g, &["-map", "[vout]", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
            let frames = out.stdout.len() / (160 * 90 * 4);
            assert!(
                out.status.success() && (frames as i64 - 30).abs() <= 2,
                "{name}: {frames} เฟรม
{}
{}",
                String::from_utf8_lossy(&out.stderr),
                g.filter
            );
        }
        for &v in Voice::ALL {
            let mut tl = base.clone();
            tl.update_clip(first, |c| {
                c.fx.voice = v;
                c.fx.fade_in = 0.5;
                // เปลี่ยนคีย์ + EQ + ลดเสียงรบกวน ซ้อนกับเสียงทุกแบบ
                if v != Voice::None {
                    c.fx.pitch = -5.0;
                    c.fx.bass = 8.0;
                    c.fx.treble = -6.0;
                    c.fx.denoise = 0.7;
                }
            });
            let g = graph::build(&tl, 0.0, 2.0, None, Some(48_000));
            let out = run_graph(&ffmpeg, &g, &["-map", "[aout]", "-f", "f32le", "-ac", "2", "pipe:1"]);
            let secs = out.stdout.len() as f64 / (48_000.0 * 8.0);
            assert!(
                out.status.success() && (secs - 2.0).abs() < 0.05,
                "voice {v:?}: {secs}
{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    /// ถอดเสียงพูดจากไทม์ไลน์ (ต้องมี whisper ในโฟลเดอร์ whisper/ แล้ว)
    #[test]
    #[ignore]
    fn transcribe_speech() {
        use crate::engine::transcribe::{Language, TranscribeJob, TranscribeOptions, TranscribeState};
        let (ffmpeg, ffprobe, media) = tools();
        let mut tl = Timeline::default();
        // ใส่เสียงพูดเริ่มที่วินาทีที่ 2 ซับต้องเลื่อนตาม
        let id = add(&mut tl, &ffprobe, &media.join("speech.wav"));
        tl.move_free(id, 2.0);
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).unwrap();
        let total = tl.duration();
        let opts = TranscribeOptions {
            language: Language::English,
            include_music: true,
            from: 0.0,
            until: total,
            max_len: 42,
            prompt: String::new(),
        };
        let job = TranscribeJob::start(ffmpeg, &tl, opts, eframe::egui::Context::default());
        let t0 = std::time::Instant::now();
        let state = loop {
            match job.state() {
                TranscribeState::Done(v) => break v,
                TranscribeState::Failed(e) => panic!("{e}"),
                _ if t0.elapsed().as_secs() > 600 => panic!("ช้าเกิน"),
                _ => std::thread::sleep(std::time::Duration::from_millis(200)),
            }
        };
        eprintln!("ใช้เวลา {:.1} วิ: {state:?}", t0.elapsed().as_secs_f64());
        let all: String = state.iter().map(|s| s.2.to_lowercase()).collect::<Vec<_>>().join(" ");
        assert!(all.contains("weather") && all.contains("beach"), "{all}");
        assert!(state[0].0 >= 1.8, "ซับแรกต้องเริ่มหลังวินาทีที่ 2: {}", state[0].0);
    }

    /// ถอดเสียงพูดไทยที่มีช่วงเงียบคั่น: เวลาซับต้องตรงช่วงพูด และบรรทัดไม่ยาวเกิน (ไฟล์ thai_raw.wav = เสียงพูดราว 20 วิ)
    #[test]
    #[ignore]
    fn transcribe_thai_timing() {
        use crate::engine::transcribe::{Language, TranscribeJob, TranscribeOptions, TranscribeState};
        let (ffmpeg, ffprobe, media) = tools();
        let mut tl = Timeline::default();
        let a = add(&mut tl, &ffprobe, &media.join("thai_raw.wav"));
        tl.move_free(a, 3.0);
        let b = add(&mut tl, &ffprobe, &media.join("thai_raw.wav"));
        tl.move_free(b, 30.0);
        let (s1, e1) = (tl.clip(a).unwrap().start, tl.clip(a).unwrap().end());
        let s2 = tl.clip(b).unwrap().start;
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).unwrap();
        let opts = TranscribeOptions {
            language: Language::Thai,
            include_music: true,
            from: 0.0,
            until: tl.duration(),
            max_len: 30,
            prompt: String::new(),
        };
        let job = TranscribeJob::start(ffmpeg, &tl, opts, eframe::egui::Context::default());
        let items = loop {
            match job.state() {
                TranscribeState::Done(v) => break v,
                TranscribeState::Failed(e) => panic!("{e}"),
                _ => std::thread::sleep(std::time::Duration::from_millis(200)),
            }
        };
        for it in &items {
            eprintln!("{:6.2} {:6.2} {}", it.0, it.1, it.2);
        }
        assert!(items.len() >= 6, "ต้องแบ่งเป็นหลายบรรทัด");
        assert!(items[0].0 >= s1 - 0.3, "ซับแรกขึ้นก่อนคนพูด {}", items[0].0);
        assert!(items.iter().all(|i| crate::core::speech::visible_len(&i.2) <= 30 && !i.2.contains('\u{FFFD}')));
        // ไม่มีซับในช่วงเงียบตรงกลาง
        assert!(items.iter().all(|i| i.1 <= e1 + 0.5 || i.0 >= s2 - 0.3), "มีซับโผล่ช่วงเงียบ");
        assert!(items.iter().any(|i| i.0 >= s2 - 0.3), "ช่วงพูดที่สองต้องมีซับ");
    }

    /// เขียนไฟล์โปรเจกต์ตัวอย่างไว้ลองเปิดในโปรแกรม
    #[test]
    #[ignore]
    fn write_sample_project() {
        let (_, ffprobe, media) = tools();
        let tl = fancy_timeline(&ffprobe, &media);
        let files = ["คลิป ทดสอบ.mp4", "rotated.mp4", "tall_noaudio.mp4", "pic.png", "song.mp3"];
        let media_list = files.iter().map(|f| media.join(f)).collect();
        crate::core::project_file::save(&media.join("ตัวอย่าง.tadtor"), &Default::default(), media_list, &tl).unwrap();
    }

    /// เล่นพรีวิว 2 วินาที (ไม่ส่งเสียงออกลำโพง) แล้วดูว่าภาพมาและนาฬิกาเดิน
    #[test]
    #[ignore]
    fn player_runs() {
        let (ffmpeg, ffprobe, media) = tools();
        let tl = sample_timeline(&ffprobe, &media);
        // เปิดลำโพงได้ (แต่ไม่ใส่แหล่งเสียง จึงเงียบ)
        let out = crate::engine::audio::AudioOut::open().expect("เปิดอุปกรณ์เสียงไม่ได้");
        assert!(out.sample_rate >= 8000);
        drop(out);

        let spec = VideoSpec { width: 960, height: 540, fps: 30 };
        let mut player = crate::engine::player::Player::start(&ffmpeg, &tl, 4.0, spec, None);
        let t0 = std::time::Instant::now();
        let (mut frames, mut last) = (0, 0.0);
        while t0.elapsed().as_secs_f64() < 3.0 {
            let up = player.update();
            frames += up.frame.is_some() as u32;
            last = up.time;
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        assert!(player.error.lock().unwrap().is_none(), "{:?}", player.error.lock().unwrap());
        // เริ่มที่ 4 วิ เล่นไปราว 2.x วิ (หักเวลารอภาพแรก)
        assert!(last > 5.5 && last < 7.2, "เวลา {last}");
        assert!(frames > 40, "ได้ภาพแค่ {frames} ภาพ");
    }

    #[test]
    #[ignore]
    fn export_mp4() {
        let (ffmpeg, ffprobe, media) = tools();
        let tl = fancy_timeline(&ffprobe, &media);
        let want = tl.duration();
        let nvenc = crate::ffmpeg::test_encoder(&ffmpeg, "h264_nvenc");
        for (encoder, name) in [(Encoder::Cpu, "out_cpu.mp4"), (Encoder::Nvenc, "out_nvenc.mp4")] {
            if encoder == Encoder::Nvenc && !nvenc {
                eprintln!("ข้าม NVENC: เครื่องนี้ใช้ไม่ได้");
                continue;
            }
            let path = media.join(name);
            let settings = ExportSettings {
                path: path.clone(),
                video: VideoSpec { width: 1280, height: 720, fps: 30 },
                encoder,
                codec: crate::engine::export::Codec::H264,
                quality: Quality::Standard,
                crf: 23,
                speed: crate::engine::export::Speed::Fast,
                audio_bitrate: 192,
            };
            let job = ExportJob::start(&ffmpeg, &tl, settings, eframe::egui::Context::default());
            let state = loop {
                match job.state() {
                    ExportState::Running { .. } => std::thread::sleep(std::time::Duration::from_millis(100)),
                    s => break s,
                }
            };
            assert!(matches!(state, ExportState::Done { .. }), "{encoder:?}: {state:?}");
            let (_, out) = info(&ffprobe, &path);
            assert!(out.has_audio && out.has_video);
            assert_eq!((out.width, out.height), (1280, 720));
            assert!((out.duration.unwrap() - want).abs() < 0.15, "{encoder:?} ยาว {:?} ควรเป็น {want}", out.duration);
        }
    }
}
