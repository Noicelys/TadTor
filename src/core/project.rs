use serde::{Deserialize, Serialize};

/// อัตราส่วนภาพของโปรเจกต์
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AspectRatio {
    Landscape16x9,
    Portrait9x16,
    Square1x1,
    Classic4x3,
    Portrait3x4,
    Cinema21x9,
}

impl AspectRatio {
    pub const ALL: [AspectRatio; 6] = [
        AspectRatio::Landscape16x9,
        AspectRatio::Portrait9x16,
        AspectRatio::Square1x1,
        AspectRatio::Classic4x3,
        AspectRatio::Portrait3x4,
        AspectRatio::Cinema21x9,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AspectRatio::Landscape16x9 => "16:9",
            AspectRatio::Portrait9x16 => "9:16",
            AspectRatio::Square1x1 => "1:1",
            AspectRatio::Classic4x3 => "4:3",
            AspectRatio::Portrait3x4 => "3:4",
            AspectRatio::Cinema21x9 => "21:9",
        }
    }

    /// ความกว้าง / ความสูง
    pub fn ratio(self) -> f32 {
        match self {
            AspectRatio::Landscape16x9 => 16.0 / 9.0,
            AspectRatio::Portrait9x16 => 9.0 / 16.0,
            AspectRatio::Square1x1 => 1.0,
            AspectRatio::Classic4x3 => 4.0 / 3.0,
            AspectRatio::Portrait3x4 => 3.0 / 4.0,
            AspectRatio::Cinema21x9 => 21.0 / 9.0,
        }
    }

    /// ขนาดภาพเมื่อด้านสั้นเท่ากับ `short_side` พิกเซล (เช่น 1080)
    pub fn resolution(self, short_side: u32) -> (u32, u32) {
        let r = self.ratio();
        let even = |v: f32| ((v / 2.0).round() as u32) * 2; // ตัวเข้ารหัสวิดีโอชอบเลขคู่
        if r >= 1.0 {
            (even(short_side as f32 * r), short_side)
        } else {
            (short_side, even(short_side as f32 / r))
        }
    }
}

/// ตั้งค่าของโปรเจกต์หนึ่งงาน
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub aspect: AspectRatio,
    /// ความละเอียดด้านสั้น เช่น 720, 1080, 1440, 2160
    pub quality: u32,
    pub fps: u32,
}

impl Default for Project {
    fn default() -> Self {
        Self { name: "โปรเจกต์ใหม่".to_owned(), aspect: AspectRatio::Landscape16x9, quality: 1080, fps: 30 }
    }
}

impl Project {
    pub fn resolution(&self) -> (u32, u32) {
        self.aspect.resolution(self.quality)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolutions() {
        assert_eq!(AspectRatio::Landscape16x9.resolution(1080), (1920, 1080));
        assert_eq!(AspectRatio::Portrait9x16.resolution(1080), (1080, 1920));
        assert_eq!(AspectRatio::Square1x1.resolution(720), (720, 720));
    }
}
