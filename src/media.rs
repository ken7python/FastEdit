use crate::model::{Asset, Rate};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub fn tool(name: &str) -> Command {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let near = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&file)));
    let mut c = Command::new(
        near.filter(|p| p.is_file())
            .unwrap_or_else(|| PathBuf::from(file)),
    );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}
pub fn probe(path: &Path) -> Result<Asset> {
    let out = tool("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .context("ffprobe を起動できません")?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr));
    }
    let data: Value = serde_json::from_slice(&out.stdout)?;
    let streams = data["streams"]
        .as_array()
        .ok_or_else(|| anyhow!("ストリームがありません"))?;
    let v = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1);
    let a = streams.iter().find(|s| s["codec_type"] == "audio");
    if v.is_none() && a.is_none() {
        bail!("未対応の素材です");
    }
    let idx = |s: &Value| s["index"].as_u64().map(|x| x as u32);
    let image = matches!(
        data["format"]["format_name"].as_str(),
        Some("image2" | "png_pipe" | "jpeg_pipe" | "webp_pipe" | "bmp_pipe")
    );
    let duration = if image {
        5.0
    } else {
        data["format"]["duration"]
            .as_str()
            .and_then(|x| x.parse::<f64>().ok())
            .filter(|x| x.is_finite() && *x > 0.0)
            .ok_or_else(|| anyhow!("素材の長さを取得できません"))?
    };
    let v = v.unwrap_or(&Value::Null);
    let size = |k: &str| v[k].as_u64().unwrap_or(0) as u32;
    let (mut width, mut height) = (size("width"), size("height"));
    if v["side_data_list"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|s| s["rotation"].as_i64().unwrap_or(0).unsigned_abs() % 180 == 90)
    {
        std::mem::swap(&mut width, &mut height);
    }
    let rate = v["avg_frame_rate"]
        .as_str()
        .and_then(|s| s.split_once('/'))
        .and_then(|(n, d)| {
            Some(Rate {
                num: n.parse().ok()?,
                den: d.parse().ok()?,
            })
        })
        .filter(|r| {
            r.num > 0 && r.den > 0 && (1.0..=240.0).contains(&(r.num as f64 / r.den as f64))
        })
        .unwrap_or(Rate { num: 30, den: 1 });
    Ok(Asset {
        id: 0,
        path: path.canonicalize()?,
        width,
        height,
        rate,
        duration,
        video: idx(v),
        audio: a.and_then(idx),
        image,
    })
}
