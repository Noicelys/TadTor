//! ลำโพง: เปิดอุปกรณ์เสียงค้างไว้ตัวเดียว แล้วสลับ "แหล่งเสียง" ตามการเล่นแต่ละครั้ง
//!
//! แหล่งเสียงเป็นคิวตัวอย่างเสียงสเตอริโอ (f32) ที่ FFmpeg เติมให้
//! จำนวนตัวอย่างที่เล่นไปแล้วใช้เป็น "นาฬิกา" ของการเล่น ภาพจึงตรงกับเสียงเสมอ

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

#[derive(Default)]
pub struct AudioSource {
    /// ตัวอย่างเสียงสลับซ้าย/ขวา
    pub buf: Mutex<VecDeque<f32>>,
    /// จำนวนเฟรมเสียง (คู่ซ้าย/ขวา) ที่ส่งออกลำโพงไปแล้ว
    pub played: AtomicU64,
    /// FFmpeg ส่งเสียงหมดแล้ว
    pub eof: AtomicBool,
    /// ยังไม่ปล่อยเสียง (รอภาพแรกของพรีวิว)
    pub hold: AtomicBool,
}

type Slot = Arc<Mutex<Option<Arc<AudioSource>>>>;

pub struct AudioOut {
    pub sample_rate: u32,
    slot: Slot,
    _stream: cpal::Stream,
}

impl AudioOut {
    pub fn open() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("ไม่พบลำโพง/อุปกรณ์เสียง")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let sample_rate = config.sample_rate;
        let channels = config.channels as usize;
        let slot: Slot = Arc::default();

        use cpal::SampleFormat as F;
        let stream = match format {
            F::F32 => build::<f32>(&device, config, channels, Arc::clone(&slot)),
            F::I16 => build::<i16>(&device, config, channels, Arc::clone(&slot)),
            F::U16 => build::<u16>(&device, config, channels, Arc::clone(&slot)),
            F::I32 => build::<i32>(&device, config, channels, Arc::clone(&slot)),
            F::F64 => build::<f64>(&device, config, channels, Arc::clone(&slot)),
            other => return Err(format!("ไม่รองรับรูปแบบเสียง {other}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { sample_rate, slot, _stream: stream })
    }

    pub fn set_source(&self, source: Option<Arc<AudioSource>>) {
        *self.slot.lock().unwrap() = source;
    }
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, channels: usize, slot: Slot) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
                let source = slot.lock().unwrap().clone();
                let silence = T::from_sample(0.0f32);
                let Some(src) = source.filter(|s| !s.hold.load(Ordering::Relaxed)) else {
                    out.fill(silence);
                    return;
                };
                let mut buf = src.buf.lock().unwrap();
                let mut frames = 0u64;
                for frame in out.chunks_mut(channels) {
                    if buf.len() < 2 {
                        frame.fill(silence);
                        continue;
                    }
                    let (l, r) = (buf.pop_front().unwrap(), buf.pop_front().unwrap());
                    frames += 1;
                    match frame.len() {
                        1 => frame[0] = T::from_sample((l + r) * 0.5),
                        _ => {
                            frame[0] = T::from_sample(l);
                            frame[1] = T::from_sample(r);
                            for s in &mut frame[2..] {
                                *s = silence;
                            }
                        }
                    }
                }
                drop(buf);
                src.played.fetch_add(frames, Ordering::Relaxed);
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}
