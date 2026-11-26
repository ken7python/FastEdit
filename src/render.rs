use crate::{media::tool, model::*};
use anyhow::{Context, Result, bail};
use std::{
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub type Cancel = Arc<AtomicBool>;
pub fn token() -> Cancel {
    Arc::new(AtomicBool::new(false))
}
pub enum Output {
    Video,
    Audio,
    Movie,
}
pub struct Plan {
    pub cmd: Command,
    pub scratch: tempfile::TempDir,
}
fn bitrate(w: u32, h: u32, r: Rate) -> String {
    format!(
        "{}",
        (w as f64 * h as f64 * 0.19 / r.seconds(1)).clamp(1_500_000., 100_000_000.) as u64
    )
}
pub fn build(p: &Project, from: i64, w: u32, h: u32, output: Output, single: bool) -> Result<Plan> {
    let scratch = tempfile::tempdir()?;
    let rate = p.rate;
    let end = if single {
        (from + 1).min(p.end())
    } else {
        p.end()
    };
    if from >= end {
        bail!("タイムラインが空です");
    }
    let seconds = rate.seconds(end - from);
    let fps = format!("{}/{}", rate.num, rate.den);
    let mut cmd = tool("ffmpeg");
    cmd.args([
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-filter_complex_threads",
        "2",
    ]);
    let video = !matches!(output, Output::Audio);
    let audio = !matches!(output, Output::Video);
    let mut filters = vec![];
    if video {
        let [r, g, b] = p.background;
        filters.push(format!(
            "color=c=0x{r:02x}{g:02x}{b:02x}:s={w}x{h}:r={fps},trim=end_frame={}[v0]",
            end - from
        ));
    }
    let mut last = String::from("v0");
    let mut sounds = vec![];
    if audio {
        filters.push(format!(
            "anullsrc=r=48000:cl=stereo,atrim=duration={seconds:.12}[sil]"
        ));
        sounds.push("[sil]".to_owned());
    }
    let mut ordered: Vec<_> = p.clips.iter().collect();
    ordered.sort_by_key(|c| c.lane);
    let mut input = 0;
    for c in ordered {
        let begin = from.max(c.start);
        let finish = end.min(c.end());
        if begin >= finish || (c.kind == Kind::Video && !video) || (c.kind == Kind::Audio && !audio)
        {
            continue;
        }
        let asset = p.assets.iter().find(|a| a.id == c.asset);
        let text_path = scratch.path().join(format!("text-{input}.png"));
        let (path, still) = if let Some(t) = &c.title {
            t.raster(p, &text_path)?;
            (text_path.as_path(), true)
        } else {
            let a = asset.context("素材が見つかりません")?;
            (a.path.as_path(), a.image)
        };
        let delay = rate.seconds(begin - from);
        let duration = rate.seconds(finish - begin);
        let seek = rate.seconds(c.offset + begin - c.start);
        if still {
            cmd.args(["-loop", "1", "-framerate", &fps]);
        } else if seek > 0.0 {
            cmd.args(["-ss", &format!("{seek:.12}")]);
        }
        cmd.args(["-threads", "2", "-t", &format!("{duration:.12}"), "-i"])
            .arg(path);
        match c.kind {
            Kind::Video => {
                let stream = asset.and_then(|a| a.video).unwrap_or(0);
                let (width, height) = c.visual_size(p);
                let sw = (width * w as f32 / p.width as f32).round().max(1.) as u32;
                let sh = (height * h as f32 / p.height as f32).round().max(1.) as u32;
                let x = c.layout.x;
                let y = c.layout.y;
                filters.push(format!("[{input}:{stream}]setpts=PTS-STARTPTS,fps={fps}:start_time=0,scale={sw}:{sh},setsar=1,setpts=PTS+{delay:.12}/TB[layer{input}]"));
                filters.push(format!("[{last}][layer{input}]overlay=x=W*{x}-w/2:y=H*{y}-h/2:eof_action=repeat:enable='between(n,{},{})'[v{}]",begin-from,finish-from-1,input+1));
                last = format!("v{}", input + 1);
            }
            Kind::Audio => {
                let stream = asset.and_then(|a| a.audio).context("音声がありません")?;
                let gain = if c.muted {
                    0.0
                } else {
                    10_f32.powf(c.gain_db / 20.)
                };
                let samples = (delay * 48000.0).round() as i64;
                filters.push(format!("[{input}:{stream}]aresample=48000,aformat=channel_layouts=stereo,volume={gain},asetpts=PTS-STARTPTS,adelay={samples}S:all=1[a{input}]"));
                sounds.push(format!("[a{input}]"));
            }
        }
        input += 1;
    }
    if audio {
        filters.push(format!(
            "{}amix=inputs={}:duration=first:normalize=0[aout]",
            sounds.join(""),
            sounds.len()
        ));
    }
    let script = scratch.path().join("filters.txt");
    std::fs::write(&script, filters.join(";\n"))?;
    cmd.arg("-filter_complex_script").arg(script);
    if video {
        cmd.args(["-map", &format!("[{last}]")]);
    }
    if audio {
        cmd.args(["-map", "[aout]"]);
    }
    match output {
        Output::Video => {
            cmd.args([
                "-threads", "1", "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1",
            ]);
        }
        Output::Audio => {
            cmd.args(["-ac", "2", "-ar", "48000", "-f", "f32le", "pipe:1"]);
        }
        Output::Movie => {
            if cfg!(target_os = "macos") {
                cmd.args([
                    "-c:v",
                    "h264_videotoolbox",
                    "-allow_sw",
                    "1",
                    "-b:v",
                    &bitrate(w, h, rate),
                ]);
            } else if cfg!(target_os = "windows") {
                cmd.args(["-c:v", "h264_mf", "-b:v", &bitrate(w, h, rate)]);
            } else {
                cmd.args([
                    "-c:v", "libx264", "-preset", "veryfast", "-threads", "2", "-crf", "19",
                ]);
            }
            cmd.args([
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-movflags",
                "+faststart",
                "-progress",
                "pipe:1",
                "-f",
                "mp4",
            ]);
        }
    }
    Ok(Plan { cmd, scratch })
}
// Drain stdout separately so cancellation can interrupt a blocked decoder.
pub fn run(
    mut plan: Plan,
    cancel: Cancel,
    read: impl FnOnce(std::process::ChildStdout) + Send + 'static,
) -> Result<()> {
    let log = tempfile::tempfile()?;
    let mut child = plan
        .cmd
        .stdout(Stdio::piped())
        .stderr(log.try_clone()?)
        .spawn()
        .context("ffmpeg を起動できません")?;
    let stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || read(stdout));
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
        }
        if let Some(s) = child.try_wait()? {
            break s;
        }
        thread::sleep(Duration::from_millis(15));
    };
    let _ = reader.join();
    drop(plan.scratch);
    if cancel.load(Ordering::Relaxed) {
        bail!("中止しました");
    }
    if !status.success() {
        use std::io::{Seek, SeekFrom};
        let mut f = log;
        f.seek(SeekFrom::Start(0))?;
        let mut msg = String::new();
        f.take(8000).read_to_string(&mut msg)?;
        bail!("FFmpeg: {msg}");
    }
    Ok(())
}
pub enum ExportEvent {
    Progress(f32),
    Done(Result<()>),
}
pub fn export(p: Project, path: &Path) -> (Cancel, mpsc::Receiver<ExportEvent>) {
    let path = path.to_owned();
    let cancel = token();
    let flag = cancel.clone();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = (|| {
            if p.width < 2
                || p.height < 2
                || !p.width.is_multiple_of(2)
                || !p.height.is_multiple_of(2)
            {
                bail!("幅と高さには 2 以上の偶数を指定してください");
            }
            let dest = std::fs::canonicalize(&path).ok();
            if dest.is_some()
                && p.assets
                    .iter()
                    .any(|a| std::fs::canonicalize(&a.path).ok() == dest)
            {
                bail!("素材と同じ場所には書き出せません");
            }
            let temp = tempfile::NamedTempFile::new_in(
                path.parent().context("保存先を確認してください")?,
            )?
            .into_temp_path();
            let mut plan = build(&p, 0, p.width, p.height, Output::Movie, false)?;
            plan.cmd.arg(&temp);
            let progress = tx.clone();
            let total = p.rate.seconds(p.end()) * 1e6;
            run(plan, flag, move |s| {
                for line in BufReader::new(s).lines().map_while(Result::ok) {
                    if let Some(n) = line
                        .strip_prefix("out_time_us=")
                        .and_then(|s| s.parse::<f64>().ok())
                    {
                        let _ = progress
                            .send(ExportEvent::Progress((n / total).clamp(0.0, 1.0) as f32));
                    }
                }
            })?;
            temp.persist(path)?;
            Ok(())
        })();
        let _ = tx.send(ExportEvent::Done(result));
    });
    (cancel, rx)
}
