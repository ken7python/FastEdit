use crate::{media::tool, model::Asset};
use eframe::egui;
use std::{collections::HashMap, io::Read, path::PathBuf, process::Stdio, sync::mpsc, thread};

#[derive(Default)]
pub struct Details {
    pub peaks: Vec<f32>,
    pub thumbnail: Option<egui::TextureHandle>,
}
pub struct Cache {
    pub entries: HashMap<PathBuf, Details>,
    request: mpsc::Sender<Asset>,
    results: mpsc::Receiver<(PathBuf, Vec<f32>, Option<egui::ColorImage>)>,
}
impl Cache {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Asset>();
        let (send, results) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(a) = rx.recv() {
                let thumb=a.video.and_then(|v|{
                let out=tool("ffmpeg").args(["-nostdin","-loglevel","error","-i"]).arg(&a.path).args(["-map",&format!("0:{v}"),"-frames:v","1","-vf","scale=144:80:force_original_aspect_ratio=decrease,pad=144:80:(ow-iw)/2:(oh-ih)/2","-threads","1","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().ok()?;
                (out.status.success() && out.stdout.len()==144*80*4).then(||egui::ColorImage::from_rgba_unmultiplied([144,80],&out.stdout))
            });
                if send.send((a.path.clone(), vec![], thumb)).is_err() {
                    break;
                }
                if let Some(stream) = a.audio
                    && let Ok(mut job) = tool("ffmpeg")
                        .args(["-nostdin", "-loglevel", "error", "-i"])
                        .arg(&a.path)
                        .args([
                            "-map",
                            &format!("0:{stream}"),
                            "-ac",
                            "1",
                            "-ar",
                            "4000",
                            "-f",
                            "s16le",
                            "pipe:1",
                        ])
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()
                {
                    let mut peaks = vec![0_f32; 512];
                    let mut n = 0u64;
                    let stride = (a.duration * 4000. / 512.).max(1.);
                    let mut out = job.stdout.take().unwrap();
                    let mut buf = [0u8; 8192];
                    let mut carry = 0;
                    while let Ok(len) = out.read(&mut buf[carry..]) {
                        if len == 0 {
                            break;
                        }
                        let used = len + carry;
                        for sample in buf[..used].chunks_exact(2) {
                            let bin = ((n as f64 / stride) as usize).min(511);
                            peaks[bin] = peaks[bin].max(
                                i16::from_le_bytes([sample[0], sample[1]]).unsigned_abs() as f32
                                    / 32768.,
                            );
                            n += 1;
                        }
                        carry = used % 2;
                        if carry > 0 {
                            buf[0] = buf[used - 1];
                        }
                    }
                    if job.wait().is_ok_and(|s| s.success())
                        && send.send((a.path, peaks, None)).is_err()
                    {
                        break;
                    }
                }
            }
        });
        Self {
            entries: HashMap::new(),
            request: tx,
            results,
        }
    }
    pub fn sync(&mut self, ctx: &egui::Context, assets: &[Asset]) {
        self.entries
            .retain(|path, _| assets.iter().any(|a| a.path == *path));
        for a in assets {
            if !self.entries.contains_key(&a.path) {
                self.entries.insert(a.path.clone(), Details::default());
                let _ = self.request.send(a.clone());
            }
        }
        while let Ok((path, peaks, thumb)) = self.results.try_recv() {
            if let Some(d) = self.entries.get_mut(&path) {
                if let Some(image) = thumb {
                    d.thumbnail = Some(ctx.load_texture("素材", image, Default::default()));
                }
                if !peaks.is_empty() {
                    d.peaks = peaks;
                }
            }
        }
    }
}
