use crate::{
    model::Project,
    render::{self, Cancel, Output},
};
use eframe::egui;
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

pub struct Device {
    _stream: OutputStream,
    handle: OutputStreamHandle,
}
impl Device {
    pub fn open() -> Option<Self> {
        OutputStream::try_default().ok().map(|(s, h)| Self {
            _stream: s,
            handle: h,
        })
    }
}
struct Pcm {
    rx: Receiver<Vec<f32>>,
    chunk: std::vec::IntoIter<f32>,
    count: Arc<AtomicU64>,
    peak: Arc<AtomicU32>,
    max: f32,
}
impl Iterator for Pcm {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.chunk.len() == 0 {
            self.peak.store(self.max.to_bits(), Ordering::Relaxed);
            self.max = 0.0;
            match self.rx.try_recv() {
                Ok(c) => self.chunk = c.into_iter(),
                Err(mpsc::TryRecvError::Empty) => return Some(0.0),
                Err(_) => return None,
            }
        }
        let v = self.chunk.next()?;
        self.max = self.max.max(v.abs());
        self.count.fetch_add(1, Ordering::Relaxed);
        Some(v)
    }
}
impl Source for Pcm {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        48000
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
pub struct Preview {
    cancel: Cancel,
    pub frames: Receiver<egui::ColorImage>,
    pub errors: Receiver<String>,
    clock: Option<Instant>,
    samples: Option<Arc<AtomicU64>>,
    pub peak: Arc<AtomicU32>,
    _sink: Option<Sink>,
    pub from: i64,
    pub displayed: i64,
    pub playing: bool,
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Preview {
    pub fn seconds(&self) -> f64 {
        self.samples
            .as_ref()
            .map(|n| n.load(Ordering::Relaxed) as f64 / 96000.0)
            .unwrap_or_else(|| self.clock.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0))
    }
    pub fn started(&mut self) {
        if self.clock.is_none() {
            self.clock = Some(Instant::now());
            if let Some(s) = &self._sink {
                s.play();
            }
        }
    }
    pub fn new(p: Project, from: i64, playing: bool, device: Option<&Device>) -> Self {
        let cancel = render::token();
        let (tx, frames) = mpsc::sync_channel(3);
        let (err, errors) = mpsc::channel();
        let ratio = (720.0 / p.width as f64)
            .min(480.0 / p.height as f64)
            .min(1.0);
        let even = |n: u32| (((n as f64 * ratio).round() as u32) / 2 * 2).max(2);
        let (w, h) = (even(p.width), even(p.height));
        let peak = Arc::new(AtomicU32::new(0));
        let sink = device
            .filter(|_| playing)
            .and_then(|d| Sink::try_new(&d.handle).ok());
        let samples = sink.as_ref().map(|s| {
            let n = Arc::new(AtomicU64::new(0));
            let (atx, rx) = mpsc::sync_channel(12);
            s.pause();
            s.append(Pcm {
                rx,
                chunk: Vec::new().into_iter(),
                count: n.clone(),
                peak: peak.clone(),
                max: 0.0,
            });
            let audio = p.clone();
            let flag = cancel.clone();
            let error = err.clone();
            thread::spawn(move || {
                let r = render::build(&audio, from, w, h, Output::Audio, false).and_then(|plan| {
                    render::run(plan, flag, move |mut stream| {
                        let mut buf = [0; 8192];
                        let mut used = 0;
                        while let Ok(n) = stream.read(&mut buf[used..]) {
                            if n == 0 {
                                break;
                            }
                            used += n;
                            if used == buf.len() {
                                if atx
                                    .send(
                                        buf.chunks_exact(4)
                                            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                                            .collect(),
                                    )
                                    .is_err()
                                {
                                    return;
                                }
                                used = 0;
                            }
                        }
                        if used > 0 {
                            let _ = atx.send(
                                buf[..used]
                                    .chunks_exact(4)
                                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                                    .collect(),
                            );
                        }
                    })
                });
                if let Err(e) = r {
                    let _ = error.send(e.to_string());
                }
            });
            n
        });
        let flag = cancel.clone();
        thread::spawn(move || {
            let r = render::build(&p, from, w, h, Output::Video, !playing).and_then(|plan| {
                render::run(plan, flag, move |mut stream| {
                    let mut buf = vec![0; w as usize * h as usize * 4];
                    while stream.read_exact(&mut buf).is_ok() {
                        if tx
                            .send(egui::ColorImage::from_rgba_unmultiplied(
                                [w as usize, h as usize],
                                &buf,
                            ))
                            .is_err()
                        {
                            break;
                        }
                    }
                })
            });
            if let Err(e) = r {
                let _ = err.send(e.to_string());
            }
        });
        Self {
            cancel,
            frames,
            errors,
            clock: None,
            samples,
            peak,
            _sink: sink,
            from,
            displayed: from - 1,
            playing,
        }
    }
}
