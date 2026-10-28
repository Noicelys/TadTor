//! ค่าการปรับแต่งของคลิป: ข้อความ, ฟิลเตอร์, ปรับสี, เอฟเฟกต์, ทรานซิชัน, เสียง
//!
//! ที่นี่เก็บแค่ "ค่า" ส่วนการแปลงเป็นคำสั่ง FFmpeg อยู่ใน ffmpeg/graph.rs

use std::ops::{Add, Mul, Sub};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// สี RGBA
pub type Rgba = [u8; 4];

macro_rules! choice_enum {
    ($(#[$m:meta])* $name:ident { $($v:ident => $label:expr),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name {
            #[default]
            $($v),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$v),+];

            pub fn label(self) -> &'static str {
                match self {
                    $($name::$v => $label),+
                }
            }
        }
    };
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub content: String,
    /// ชื่อฟอนต์ (ว่าง = ฟอนต์ไทยของ Windows) ดูรายชื่อได้จาก `crate::fonts`
    pub font: String,
    /// ขนาดตัวอักษร (% ของความสูงภาพ)
    pub size: f32,
    pub bold: bool,
    /// ใช้ได้เฉพาะฟอนต์ที่มีแบบตัวเอียง
    pub italic: bool,
    pub color: Rgba,
    /// ความทึบทั้งข้อความ 0..1 (รวมขอบ เงา กล่อง)
    pub opacity: f32,
    pub align: TextAlign,
    /// ระยะห่างบรรทัด (% ของขนาดตัวอักษร)
    pub line_spacing: f32,
    /// ความหนาขอบ (% ของขนาดตัวอักษร)
    pub outline: f32,
    pub outline_color: Rgba,
    pub shadow: bool,
    pub shadow_color: Rgba,
    /// ระยะเงา (% ของขนาดตัวอักษร)
    pub shadow_offset: f32,
    /// ทิศเงา (องศา) 0 = ขวา, 90 = ล่าง, 180 = ซ้าย, 270 = บน
    pub shadow_angle: f32,
    pub background: bool,
    pub background_color: Rgba,
    /// ขอบในกล่องพื้นหลัง (% ของขนาดตัวอักษร)
    pub box_padding: f32,
    /// ตำแหน่งกึ่งกลางข้อความ (0..1 ของกว้าง/สูง)
    pub x: f32,
    pub y: f32,
    pub motion: Motion,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            content: "ข้อความ".to_owned(),
            font: String::new(),
            size: 7.0,
            bold: true,
            italic: false,
            color: [255, 255, 255, 255],
            opacity: 1.0,
            align: TextAlign::Center,
            line_spacing: 15.0,
            outline: 8.0,
            outline_color: [0, 0, 0, 255],
            shadow: false,
            shadow_color: [0, 0, 0, 150],
            shadow_offset: 6.0,
            shadow_angle: 45.0,
            background: false,
            background_color: [0, 0, 0, 160],
            box_padding: 30.0,
            x: 0.5,
            y: 0.5,
            motion: Motion::default(),
        }
    }
}

impl TextStyle {
    /// ระยะเงาแนวนอน/แนวตั้ง (% ของขนาดตัวอักษร) ที่ 45° ได้ค่าเท่า `shadow_offset` ทั้งสองแกน
    pub fn shadow_xy(&self) -> (f32, f32) {
        let a = self.shadow_angle.to_radians();
        let d = self.shadow_offset * std::f32::consts::SQRT_2;
        (d * a.cos(), d * a.sin())
    }

    /// แบบซับไตเติล: ตัวเล็กลง ล่างจอ มีกล่องพื้นหลัง
    pub fn subtitle(content: String) -> Self {
        Self { content, size: 5.0, bold: false, outline: 0.0, background: true, y: 0.88, ..Self::default() }
    }

    /// สไตล์สำเร็จรูปในแท็บข้อความ
    pub fn presets() -> Vec<(&'static str, TextStyle)> {
        let d = TextStyle::default();
        let m = |anim_in, anim_out, looping| Motion { anim_in, anim_out, looping, ..Motion::default() };
        vec![
            ("ขาวขอบดำ", d.clone()),
            ("หัวข้อใหญ่", TextStyle { size: 12.0, outline: 6.0, y: 0.4, ..d.clone() }),
            ("กล่องดำ", TextStyle { bold: false, outline: 0.0, background: true, ..d.clone() }),
            ("เหลืองเด่น", TextStyle { color: [255, 214, 10, 255], outline: 10.0, ..d.clone() }),
            ("ซับไตเติล", TextStyle::subtitle("ซับไตเติล".to_owned())),
            ("มุมล่างซ้าย", TextStyle { size: 4.5, bold: false, outline: 6.0, x: 0.2, y: 0.9, ..d.clone() }),
            // แบบมีภาพเคลื่อนไหว
            (
                "หัวข้อเด้ง",
                TextStyle { size: 11.0, outline: 6.0, y: 0.4, motion: m(TextMotion::Pop, TextMotion::Shrink, TextLoop::None), ..d.clone() },
            ),
            (
                "ป้ายเหลืองเต้น",
                TextStyle {
                    color: [20, 20, 20, 255],
                    outline: 0.0,
                    background: true,
                    background_color: [255, 214, 10, 255],
                    box_padding: 40.0,
                    motion: m(TextMotion::Grow, TextMotion::Fade, TextLoop::Pulse),
                    ..d.clone()
                },
            ),
            (
                "ซับลอยขึ้น",
                TextStyle {
                    motion: m(TextMotion::Up, TextMotion::Fade, TextLoop::None), ..TextStyle::subtitle("ซับไตเติล".to_owned())
                },
            ),
            (
                "เงานุ่มลอย",
                TextStyle {
                    outline: 0.0,
                    shadow: true,
                    shadow_offset: 8.0,
                    motion: m(TextMotion::Fade, TextMotion::Fade, TextLoop::Float),
                    ..d.clone()
                },
            ),
            (
                "เข้าจากขวา",
                TextStyle {
                    size: 6.0,
                    outline: 0.0,
                    background: true,
                    background_color: [229, 57, 53, 230],
                    x: 0.3,
                    y: 0.78,
                    motion: m(TextMotion::Left, TextMotion::Right, TextLoop::None),
                    ..d.clone()
                },
            ),
            (
                "ตกใจสั่น",
                TextStyle {
                    color: [255, 82, 82, 255],
                    size: 10.0,
                    outline: 10.0,
                    motion: m(TextMotion::Pop, TextMotion::Fade, TextLoop::Shake),
                    ..d
                },
            ),
        ]
    }
}

choice_enum!(
    /// จัดบรรทัดของข้อความหลายบรรทัด
    TextAlign {
        Center => "กลาง",
        Left => "ซ้าย",
        Right => "ขวา",
    }
);

choice_enum!(
    /// ภาพเคลื่อนไหวตอนข้อความเข้า/ออก (ตัวแรกคือไม่มี)
    /// ทิศทางนับตามการเคลื่อนที่: "เลื่อนขึ้น" ตอนเข้า = โผล่จากด้านล่างขึ้นมา ตอนออก = ลอยขึ้นหายไป
    TextMotion {
        None => "ไม่มี",
        Fade => "จาง",
        Up => "เลื่อนขึ้น",
        Down => "เลื่อนลง",
        Left => "เลื่อนซ้าย",
        Right => "เลื่อนขวา",
        Grow => "ซูมขยาย",
        Shrink => "ซูมหด",
        Pop => "เด้ง",
    }
);

choice_enum!(
    /// ภาพเคลื่อนไหววนซ้ำตลอดคลิป
    TextLoop {
        None => "ไม่มี",
        Float => "ลอย",
        Pulse => "เต้นตุบ",
        Sway => "โยก",
        Shake => "สั่น",
        Blink => "กระพริบ",
    }
);

choice_enum!(
    /// จังหวะของท่าเข้า/ออก ("เด้ง" ใช้จังหวะของตัวเองเสมอ)
    Easing {
        Smooth => "นุ่ม",
        Linear => "คงที่",
        SmoothBoth => "นุ่มหัวท้าย",
        Back => "เลยแล้วกลับ",
    }
);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Motion {
    pub anim_in: TextMotion,
    /// วินาที
    pub in_dur: f32,
    pub anim_out: TextMotion,
    pub out_dur: f32,
    pub easing: Easing,
    /// ระยะเลื่อน/ซูมของท่าเข้า-ออก 1 = ปกติ
    pub strength: f32,
    pub looping: TextLoop,
    /// 1 = ปกติ
    pub loop_speed: f32,
    /// ความแรงของท่าวนซ้ำ 1 = ปกติ
    pub loop_amount: f32,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            anim_in: TextMotion::None,
            in_dur: 0.5,
            anim_out: TextMotion::None,
            out_dur: 0.5,
            easing: Easing::Smooth,
            strength: 1.0,
            looping: TextLoop::None,
            loop_speed: 1.0,
            loop_amount: 1.0,
        }
    }
}

/// ตัวเลขที่ใช้คำนวณภาพเคลื่อนไหว: `f32` (ตัวอย่างในหน้าจอ) หรือสูตรของ FFmpeg (ตอนเรนเดอร์)
/// เขียนสูตรครั้งเดียวใน `Motion::pose_at` ภาพในการ์ดตัวอย่างจึงตรงกับวิดีโอจริงเสมอ
pub trait MotionNum: Clone + From<f32> + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> {
    fn sin(self) -> Self;
    /// บีบให้อยู่ในช่วง 0..1
    fn clamp01(self) -> Self;
}

impl MotionNum for f32 {
    fn sin(self) -> Self {
        f32::sin(self)
    }
    fn clamp01(self) -> Self {
        self.clamp(0.0, 1.0)
    }
}

/// ท่าของข้อความ ณ เวลาหนึ่ง: เลื่อน (หน่วย = สัดส่วนความสูงภาพ), ขนาด (1 = ปกติ), ความทึบ
pub struct Pose<N> {
    pub dx: N,
    pub dy: N,
    pub scale: N,
    pub alpha: N,
}

impl TextMotion {
    /// ท่าตอนเริ่มเข้า (`entering`) หรือตอนออกจบ เป็น (dx, dy, scale, alpha)
    /// `k` = ความแรง (คูณระยะเลื่อนและระยะซูม)
    fn pose(self, entering: bool, k: f32) -> (f32, f32, f32, f32) {
        let (x, y, s, a) = self.base_pose(entering);
        (x * k, y * k, (1.0 + (s - 1.0) * k).max(0.0), a)
    }

    fn base_pose(self, entering: bool) -> (f32, f32, f32, f32) {
        let d = if entering { 1.0 } else { -1.0 };
        match self {
            TextMotion::None => (0.0, 0.0, 1.0, 1.0),
            TextMotion::Fade => (0.0, 0.0, 1.0, 0.0),
            TextMotion::Up => (0.0, 0.08 * d, 1.0, 0.0),
            TextMotion::Down => (0.0, -0.08 * d, 1.0, 0.0),
            TextMotion::Left => (0.15 * d, 0.0, 1.0, 0.0),
            TextMotion::Right => (-0.15 * d, 0.0, 1.0, 0.0),
            TextMotion::Grow => (0.0, 0.0, if entering { 0.4 } else { 1.6 }, 0.0),
            TextMotion::Shrink => (0.0, 0.0, if entering { 1.6 } else { 0.3 }, 0.0),
            TextMotion::Pop => (0.0, 0.0, 0.0, 0.0),
        }
    }

    /// น้ำหนักของท่าเริ่ม/จบ จาก `p` = 0 (อยู่ที่ท่าเริ่ม) ถึง 1 (ท่าปกติ)
    fn weight<N: MotionNum>(self, p: N, easing: Easing) -> N {
        let u = N::from(1.0) - p;
        let easing = if self == TextMotion::Pop { Easing::Back } else { easing };
        match easing {
            // ease-out-cubic
            Easing::Smooth => u.clone() * u.clone() * u,
            Easing::Linear => u,
            // smoothstep: ช้าตอนเริ่มและตอนจบ
            Easing::SmoothBoth => u.clone() * u.clone() * (N::from(3.0) - N::from(2.0) * u),
            // ease-out-back: เลยท่าปกติไปนิดแล้วเด้งกลับ
            Easing::Back => {
                let (c1, c3) = (1.70158, 2.70158);
                u.clone() * u.clone() * (N::from(c3) * u - N::from(c1))
            }
        }
    }
}

impl Motion {
    pub fn is_static(&self) -> bool {
        self.anim_in == TextMotion::None && self.anim_out == TextMotion::None && self.looping == TextLoop::None
    }

    /// ความยาวจริงของช่วงเข้า/ออก (ไม่ให้เกินความยาวคลิป ถ้ามีทั้งคู่ได้คนละครึ่ง)
    fn durations(&self, clip_dur: f32) -> (f32, f32) {
        let both = self.anim_in != TextMotion::None && self.anim_out != TextMotion::None;
        let cap = if both { clip_dur / 2.0 } else { clip_dur };
        (self.in_dur.clamp(0.05, cap.max(0.05)), self.out_dur.clamp(0.05, cap.max(0.05)))
    }

    /// ท่าของข้อความที่เวลา `t` (วินาทีนับจากหัวคลิป) ในคลิปยาว `clip_dur`
    pub fn pose_at<N: MotionNum>(&self, t: N, clip_dur: f32) -> Pose<N> {
        let (din, dout) = self.durations(clip_dur);
        let mut dx = N::from(0.0);
        let mut dy = N::from(0.0);
        let mut scale = N::from(1.0);
        let mut alpha = N::from(1.0);
        let one = || N::from(1.0);
        // p = ความคืบหน้าของช่วงเข้า (นับจากหัวคลิป) และช่วงออก (นับถอยจากท้ายคลิป)
        let phases = [
            (self.anim_in, true, (t.clone() * N::from(1.0 / din)).clamp01()),
            (self.anim_out, false, ((N::from(clip_dur) - t.clone()) * N::from(1.0 / dout)).clamp01()),
        ];
        for (kind, entering, p) in phases {
            if kind == TextMotion::None {
                continue;
            }
            let (x, y, s, a) = kind.pose(entering, self.strength);
            let w = kind.weight(p, self.easing);
            if x != 0.0 {
                dx = dx + N::from(x) * w.clone();
            }
            if y != 0.0 {
                dy = dy + N::from(y) * w.clone();
            }
            if s != 1.0 {
                scale = scale * (one() + N::from(s - 1.0) * w.clone());
            }
            if a != 1.0 {
                alpha = alpha * (one() + N::from(a - 1.0) * w);
            }
        }
        // วนซ้ำ: คลื่น sin ตามเวลา
        let wave = |period: f32, phase: f32| (t.clone() * N::from(std::f32::consts::TAU * self.loop_speed / period) + N::from(phase)).sin();
        let k = self.loop_amount.max(0.0);
        match self.looping {
            TextLoop::None => {}
            TextLoop::Float => dy = dy + N::from(0.012 * k) * wave(1.6, 0.0),
            TextLoop::Pulse => scale = scale * (one() + N::from(0.06 * k) * wave(0.8, 0.0)),
            TextLoop::Sway => dx = dx + N::from(0.015 * k) * wave(1.6, 0.0),
            TextLoop::Shake => {
                dx = dx + N::from(0.005 * k) * wave(0.11, 0.0);
                dy = dy + N::from(0.005 * k) * wave(0.13, 1.0);
            }
            // k = 1: ทึบ 0.2..1 ยิ่งแรงยิ่งจางลึก (ถึงหายหมดได้)
            TextLoop::Blink => alpha = alpha * (one() - N::from(0.4 * k) * (one() - wave(0.9, std::f32::consts::FRAC_PI_2))).clamp01(),
        }
        Pose { dx, dy, scale, alpha }
    }
}

choice_enum!(
    /// ฟิลเตอร์สำเร็จรูป (ตัวแรกคือไม่มี)
    FilterPreset {
        None => "ไม่มี",
        Vivid => "สดใส",
        Warm => "อบอุ่น",
        Cool => "เย็นตา",
        Film => "ภาพยนตร์",
        Vintage => "วินเทจ",
        Fade => "จางนุ่ม",
        Sepia => "ซีเปีย",
        Mono => "ขาวดำ",
        Noir => "ขาวดำเข้ม",
    }
);

choice_enum!(
    Effect {
        None => "ไม่มี",
        Vignette => "ขอบมืด",
        Blur => "เบลอ",
        Sharpen => "คมชัด",
        Grain => "เม็ดฟิล์ม",
        OldFilm => "ฟิล์มเก่า",
        Cinema => "แถบดำภาพยนตร์",
        Mirror => "กลับซ้ายขวา",
        Glow => "ฟุ้งแสง",
    }
);

choice_enum!(
    TransitionKind {
        Fade => "จางเข้าหากัน",
        Dissolve => "ละลาย",
        FadeBlack => "ผ่านจอดำ",
        FadeWhite => "ผ่านจอขาว",
        WipeLeft => "กวาดซ้าย",
        WipeRight => "กวาดขวา",
        SlideLeft => "เลื่อนซ้าย",
        SlideUp => "เลื่อนขึ้น",
        SmoothLeft => "ลื่นซ้าย",
        CircleOpen => "วงกลมเปิด",
        CircleClose => "วงกลมปิด",
        Radial => "หมุนกวาด",
        Pixelize => "แตกพิกเซล",
        ZoomIn => "ซูมเข้า",
        HBlur => "เบลอแนวนอน",
        WipeUp => "กวาดขึ้น",
        WipeDown => "กวาดลง",
        SlideRight => "เลื่อนขวา",
        SlideDown => "เลื่อนลง",
        SmoothRight => "ลื่นขวา",
        SmoothUp => "ลื่นขึ้น",
        SmoothDown => "ลื่นลง",
        CircleCrop => "วงกลมครอป",
        RectCrop => "กรอบสี่เหลี่ยม",
        HorzOpen => "เปิดแนวนอน",
        VertOpen => "เปิดแนวตั้ง",
        DiagTl => "ทแยงมุม",
        SliceLeft => "ซี่ซ้าย",
        SqueezeH => "บีบแนวนอน",
        FadeGrays => "จางผ่านขาวดำ",
        Distance => "บิดละลาย",
        CoverLeft => "ทับจากขวา",
        RevealLeft => "เปิดเผยซ้าย",
        WindLeft => "ลมพัดซ้าย",
    }
);

choice_enum!(
    /// วิธีวางภาพลงจอเมื่ออัตราส่วนไม่ตรงกับโปรเจกต์
    Fit {
        Contain => "พอดีจอ",
        Cover => "เต็มจอ",
        Stretch => "ยืดเต็ม",
    }
);

choice_enum!(
    /// เปลี่ยนเสียง
    Voice {
        None => "ปกติ",
        Chipmunk => "ชิปมังก์",
        Deep => "เสียงทุ้ม",
        Robot => "หุ่นยนต์",
        Phone => "โทรศัพท์",
        Echo => "ก้อง",
        Hall => "ห้องโถง",
    }
);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransitionKind,
    /// วินาที (ของจริงอาจสั้นกว่านี้ถ้าคลิปสั้น)
    pub dur: f64,
}

pub const TRANSITION_DEFAULT: f64 = 0.6;

/// การปรับแต่งของคลิป ค่าเริ่มต้น = ไม่เปลี่ยนอะไร
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipFx {
    // ---- เสียง ----
    /// 0..2 (1 = เท่าเดิม)
    pub volume: f32,
    pub fade_in: f64,
    pub fade_out: f64,
    pub voice: Voice,
    /// เปลี่ยนคีย์ (ครึ่งเสียง) -12..12
    pub pitch: f32,
    /// เสียงทุ้ม/แหลม (dB) -12..12
    pub bass: f32,
    pub treble: f32,
    /// ลดเสียงรบกวน 0..1
    pub denoise: f32,
    // ---- จัดวางภาพ ----
    pub fit: Fit,
    /// 1 = ขนาดตามแบบวาง
    pub zoom: f32,
    /// เลื่อนภาพ -1..1 (บวก = ไปขวา/ลง) มีผลเมื่อภาพใหญ่หรือเล็กกว่าจอ
    pub pan_x: f32,
    pub pan_y: f32,
    /// หมุนทีละ 90° ตามเข็มนาฬิกา (0..3)
    pub rotate: u8,
    pub flip_h: bool,
    pub flip_v: bool,
    /// สีพื้นที่ว่างรอบภาพ
    pub background: Rgba,
    // ---- ภาพ ----
    pub filter: FilterPreset,
    /// ความเข้มฟิลเตอร์ 0..1
    pub filter_amount: f32,
    /// ค่าปรับสีทั้งหมดอยู่ในช่วง -1..1 (0 = ไม่ปรับ)
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature: f32,
    /// 0..1
    pub sharpen: f32,
    pub vignette: f32,
    /// เฉดสี -1..1 (= -180°..180°)
    pub hue: f32,
    /// เขียว (-) ↔ ม่วงแดง (+)
    pub tint: f32,
    /// ส่วนสว่าง / ส่วนมืด -1..1
    pub highlights: f32,
    pub shadows: f32,
    /// ยกจุดดำให้ภาพจางแบบฟิล์ม 0..1
    pub fade: f32,
    /// เม็ดฟิล์ม 0..1
    pub grain: f32,
    pub lut: Option<PathBuf>,
    pub effect: Effect,
    /// ความแรงของเอฟเฟกต์ 0..2 (1 = ปกติ)
    pub effect_amount: f32,
    /// ทรานซิชันจากคลิปนี้ไปคลิปถัดไป (แทร็กหลักเท่านั้น)
    pub transition: Option<Transition>,
}

impl Default for ClipFx {
    fn default() -> Self {
        Self {
            volume: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            voice: Voice::None,
            pitch: 0.0,
            bass: 0.0,
            treble: 0.0,
            denoise: 0.0,
            fit: Fit::Contain,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            rotate: 0,
            flip_h: false,
            flip_v: false,
            background: [0, 0, 0, 255],
            filter: FilterPreset::None,
            filter_amount: 1.0,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            temperature: 0.0,
            sharpen: 0.0,
            vignette: 0.0,
            hue: 0.0,
            tint: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            fade: 0.0,
            grain: 0.0,
            lut: None,
            effect: Effect::None,
            effect_amount: 1.0,
            transition: None,
        }
    }
}

impl ClipFx {
    fn adjust_values(&self) -> [f32; 12] {
        [
            self.brightness,
            self.contrast,
            self.saturation,
            self.temperature,
            self.sharpen,
            self.vignette,
            self.hue,
            self.tint,
            self.highlights,
            self.shadows,
            self.fade,
            self.grain,
        ]
    }

    pub fn has_adjust(&self) -> bool {
        self.adjust_values().iter().any(|v| v.abs() > 1e-3)
    }

    pub fn reset_adjust(&mut self) {
        let d = ClipFx::default();
        self.brightness = d.brightness;
        self.contrast = d.contrast;
        self.saturation = d.saturation;
        self.temperature = d.temperature;
        self.sharpen = d.sharpen;
        self.vignette = d.vignette;
        self.hue = d.hue;
        self.tint = d.tint;
        self.highlights = d.highlights;
        self.shadows = d.shadows;
        self.fade = d.fade;
        self.grain = d.grain;
        self.lut = None;
    }

    /// จัดวางภาพต่างจากค่าเริ่มต้นหรือไม่
    pub fn has_layout(&self) -> bool {
        let d = ClipFx::default();
        self.fit != d.fit
            || (self.zoom - d.zoom).abs() > 1e-3
            || self.pan_x.abs() > 1e-3
            || self.pan_y.abs() > 1e-3
            || !self.rotate.is_multiple_of(4)
            || self.flip_h
            || self.flip_v
            || self.background != d.background
    }

    pub fn reset_layout(&mut self) {
        let d = ClipFx::default();
        self.fit = d.fit;
        self.zoom = d.zoom;
        self.pan_x = d.pan_x;
        self.pan_y = d.pan_y;
        self.rotate = d.rotate;
        self.flip_h = d.flip_h;
        self.flip_v = d.flip_v;
        self.background = d.background;
    }

    /// ปรับแต่งเสียงต่างจากค่าเริ่มต้นหรือไม่ (ไม่นับระดับเสียงและเฟด)
    pub fn has_sound_fx(&self) -> bool {
        self.voice != Voice::None || [self.pitch, self.bass, self.treble, self.denoise].iter().any(|v| v.abs() > 1e-3)
    }

    pub fn reset_sound_fx(&mut self) {
        let d = ClipFx::default();
        self.voice = d.voice;
        self.pitch = d.pitch;
        self.bass = d.bass;
        self.treble = d.treble;
        self.denoise = d.denoise;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn motion_starts_and_ends_at_pose() {
        let m = Motion { anim_in: TextMotion::Up, anim_out: TextMotion::Shrink, ..Motion::default() };
        // หัวคลิป: อยู่ล่างและโปร่งใส
        let p = m.pose_at(0.0f32, 3.0);
        assert!(near(p.dy, 0.08) && near(p.alpha, 0.0));
        // กลางคลิป: ท่าปกติ
        let p = m.pose_at(1.5f32, 3.0);
        assert!(near(p.dx, 0.0) && near(p.dy, 0.0) && near(p.scale, 1.0) && near(p.alpha, 1.0));
        // ท้ายคลิป: หดและหายไป
        let p = m.pose_at(3.0f32, 3.0);
        assert!(near(p.scale, 0.3) && near(p.alpha, 0.0));
    }

    #[test]
    fn pop_overshoots() {
        let m = Motion { anim_in: TextMotion::Pop, ..Motion::default() };
        let peak = (1..50).map(|i| m.pose_at(i as f32 * 0.01, 3.0).scale).fold(0.0, f32::max);
        assert!(peak > 1.05, "เด้งต้องใหญ่เกินขนาดจริงนิดหนึ่ง ได้ {peak}");
        assert!(near(m.pose_at(0.5f32, 3.0).scale, 1.0));
    }

    #[test]
    fn short_clip_splits_in_and_out() {
        let m = Motion { anim_in: TextMotion::Fade, anim_out: TextMotion::Fade, in_dur: 2.0, out_dur: 2.0, ..Motion::default() };
        // คลิป 1 วิ: เข้า 0.5 ออก 0.5 กลางคลิปต้องเห็นเต็มที่
        assert!(near(m.pose_at(0.5f32, 1.0).alpha, 1.0));
    }

    #[test]
    fn strength_scales_motion() {
        let m = Motion { anim_in: TextMotion::Up, strength: 2.0, ..Motion::default() };
        assert!(near(m.pose_at(0.0f32, 3.0).dy, 0.16));
        let m = Motion { anim_in: TextMotion::Grow, strength: 0.5, ..Motion::default() };
        assert!(near(m.pose_at(0.0f32, 3.0).scale, 0.7));
    }

    #[test]
    fn easing_changes_midpoint() {
        let at = |easing| Motion { anim_in: TextMotion::Up, easing, ..Motion::default() }.pose_at(0.25f32, 3.0).dy;
        // ครึ่งทาง (0.25 จาก 0.5 วิ): คงที่ = เหลือครึ่งระยะ, นุ่ม = เหลือ 1/8, นุ่มหัวท้าย = ครึ่งพอดี
        assert!(near(at(Easing::Linear), 0.04));
        assert!(near(at(Easing::Smooth), 0.01));
        assert!(near(at(Easing::SmoothBoth), 0.04));
    }

    #[test]
    fn default_shadow_matches_old_offset() {
        let t = TextStyle::default();
        let (x, y) = t.shadow_xy();
        assert!(near(x, t.shadow_offset) && near(y, t.shadow_offset));
    }

    #[test]
    fn old_project_text_loads() {
        let old = r#"{"content":"hi","size":5.0,"bold":false,"color":[1,2,3,255],"outline":0.0,
            "outline_color":[0,0,0,255],"background":true,"background_color":[0,0,0,160],"x":0.5,"y":0.9}"#;
        let t: TextStyle = serde_json::from_str(old).unwrap();
        assert_eq!(t.content, "hi");
        assert!(t.motion.is_static() && t.font.is_empty() && near(t.opacity, 1.0));
    }
}
