use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct Rate {
    pub num: u32,
    pub den: u32,
}
impl Rate {
    pub fn seconds(self, frames: i64) -> f64 {
        frames as f64 * self.den as f64 / self.num as f64
    }
    pub fn frames(self, seconds: f64) -> i64 {
        (seconds * self.num as f64 / self.den as f64).round() as i64
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Asset {
    pub id: u64,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub rate: Rate,
    pub duration: f64,
    pub video: Option<u32>,
    pub audio: Option<u32>,
    pub image: bool,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum Kind {
    Video,
    Audio,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Clip {
    pub id: u64,
    pub asset: u64,
    pub kind: Kind,
    pub lane: u32,
    pub start: i64,
    pub len: i64,
    pub offset: i64,
    pub link: Option<u64>,
}
impl Clip {
    pub fn end(&self) -> i64 {
        self.start + self.len
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Project {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub rate: Rate,
    pub assets: Vec<Asset>,
    pub clips: Vec<Clip>,
    pub next: u64,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            version: 1,
            width: 1920,
            height: 1080,
            rate: Rate { num: 30, den: 1 },
            assets: vec![],
            clips: vec![],
            next: 1,
        }
    }
}
impl Project {
    pub fn id(&mut self) -> u64 {
        let n = self.next;
        self.next += 1;
        n
    }
    pub fn end(&self) -> i64 {
        self.clips.iter().map(Clip::end).max().unwrap_or(0)
    }
    pub fn place(&mut self, asset: &Asset, start: i64, lane: u32) -> u64 {
        let len = self.rate.frames(asset.duration).max(1);
        let group = self.id();
        let mut first = 0;
        for (exists, kind) in [
            (asset.video.is_some(), Kind::Video),
            (asset.audio.is_some(), Kind::Audio),
        ] {
            if exists {
                let id = self.id();
                if first == 0 {
                    first = id;
                }
                self.clips.push(Clip {
                    id,
                    asset: asset.id,
                    kind,
                    lane,
                    start,
                    len,
                    offset: 0,
                    link: Some(group),
                });
            }
        }
        first
    }
}
