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
    #[serde(default)]
    pub layout: crate::layers::Layout,
    #[serde(default)]
    pub title: Option<crate::layers::Title>,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub muted: bool,
}
impl Clip {
    pub fn end(&self) -> i64 {
        self.start + self.len
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Project {
    pub version: u32,
    #[serde(default)]
    pub background: [u8; 3],
    pub width: u32,
    pub height: u32,
    pub rate: Rate,
    #[serde(default)]
    pub extra_video_tracks: u32,
    pub assets: Vec<Asset>,
    pub clips: Vec<Clip>,
    pub next: u64,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            version: 1,
            background: [0; 3],
            width: 1920,
            height: 1080,
            rate: Rate { num: 30, den: 1 },
            extra_video_tracks: 0,
            assets: vec![],
            clips: vec![],
            next: 1,
        }
    }
}
impl Project {
    pub fn highest_video_track(&self) -> u32 {
        self.clips
            .iter()
            .filter(|c| c.kind == Kind::Video)
            .map(|c| c.lane)
            .max()
            .unwrap_or(0)
            .max(self.extra_video_tracks)
    }
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
                    layout: Default::default(),
                    title: None,
                    gain_db: 0.,
                    muted: false,
                });
            }
        }
        first
    }
}

impl Project {
    pub fn clip(&self, id: u64) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }
    fn group(&self, id: u64) -> Vec<u64> {
        let link = self.clip(id).and_then(|c| c.link);
        self.clips
            .iter()
            .filter(|c| c.id == id || (link.is_some() && c.link == link))
            .map(|c| c.id)
            .collect()
    }
    fn split_ids(&mut self, ids: &[u64], at: i64) {
        use std::collections::HashMap;
        let mut right_links = HashMap::new();
        for &id in ids {
            let Some(i) = self
                .clips
                .iter()
                .position(|c| c.id == id && c.start < at && at < c.end())
            else {
                continue;
            };
            let mut right = self.clips[i].clone();
            let left_len = at - right.start;
            self.clips[i].len = left_len;
            right.id = self.id();
            right.start = at;
            right.offset += left_len;
            right.len -= left_len;
            right.link = right
                .link
                .map(|link| *right_links.entry(link).or_insert_with(|| self.id()));
            self.clips.push(right);
        }
    }
    pub fn split(&mut self, id: u64, at: i64) {
        self.split_ids(&self.group(id), at);
    }
    pub fn delete(&mut self, id: u64, ripple: bool) {
        let Some(c) = self.clip(id).cloned() else {
            return;
        };
        if ripple {
            for at in [c.start, c.end()] {
                let ids: Vec<_> = self.clips.iter().map(|x| x.id).collect();
                self.split_ids(&ids, at);
            }
            self.clips
                .retain(|x| x.start < c.start || x.start >= c.end());
            for x in &mut self.clips {
                if x.start >= c.end() {
                    x.start -= c.len;
                }
            }
        } else {
            let group = self.group(id);
            self.clips.retain(|x| !group.contains(&x.id));
        }
    }
    pub fn adjust(&mut self, id: u64, mode: Handle, delta: i64) {
        let ids = self.group(id);
        let mut delta = delta;
        for c in self.clips.iter().filter(|c| ids.contains(&c.id)) {
            delta = match mode {
                Handle::Body => delta.max(-c.start),
                Handle::Left => delta.clamp((-c.start).max(-c.offset), c.len - 1),
                Handle::Right => {
                    let limit = self
                        .assets
                        .iter()
                        .find(|a| a.id == c.asset && !a.image)
                        .map_or(i64::MAX / 4, |a| {
                            self.rate.frames(a.duration) - c.offset - c.len
                        });
                    delta.clamp(1 - c.len, limit.max(0))
                }
            };
        }
        for c in self.clips.iter_mut().filter(|c| ids.contains(&c.id)) {
            match mode {
                Handle::Body => c.start += delta,
                Handle::Left => {
                    c.start += delta;
                    c.len -= delta;
                    c.offset += delta;
                }
                Handle::Right => c.len += delta,
            }
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
pub enum Handle {
    Body,
    Left,
    Right,
}
