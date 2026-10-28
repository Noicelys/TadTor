//! ย้อนกลับ / ทำซ้ำ ด้วยการเก็บสำเนาไทม์ไลน์ทั้งก้อน (ไทม์ไลน์เล็กมาก ทำแบบนี้ง่ายและไม่พลาด)

use super::timeline::Timeline;

const LIMIT: usize = 200;

#[derive(Default)]
pub struct History {
    undo: Vec<Timeline>,
    redo: Vec<Timeline>,
}

impl History {
    /// เรียกก่อนแก้ไทม์ไลน์ ส่งสถานะเดิมมาเก็บไว้
    pub fn push(&mut self, before: Timeline) {
        if self.undo.last() == Some(&before) {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &mut Timeline) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(current, prev));
        true
    }

    pub fn redo(&mut self, current: &mut Timeline) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(current, next));
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}
