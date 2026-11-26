use crate::{Editor, model::*};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::{collections::HashSet, io::Write, path::Path};

fn save(path: &Path, project: &Project) -> Result<()> {
    if let Ok(dest) = path.canonicalize() {
        ensure!(
            !project
                .assets
                .iter()
                .any(|a| a.path.canonicalize().ok().as_ref() == Some(&dest)),
            "素材自身には保存できません"
        );
    }
    let dir = path.parent().context("保存先フォルダーがありません")?;
    let mut p = project.clone();
    for a in &mut p.assets {
        if let Some(rel) = pathdiff::diff_paths(&a.path, dir) {
            #[cfg(windows)]
            let rel = std::path::PathBuf::from(rel.to_string_lossy().replace('\\', "/"));
            a.path = rel;
        }
    }
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer_pretty(&mut file, &p)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}
fn load(path: &Path) -> Result<Project> {
    let data: serde_json::Value = serde_json::from_reader(std::fs::File::open(path)?)?;
    ensure!(
        data["version"] == 1,
        "このプロジェクト形式には対応していません"
    );
    let mut p: Project = serde_json::from_value(data)?;
    ensure!(
        p.extra_video_tracks < 1024
            && (2..=16384).contains(&p.width)
            && (2..=16384).contains(&p.height)
            && p.rate.num > 0
            && p.rate.den > 0,
        "画面設定を読み込めません"
    );
    let mut ids = HashSet::new();
    for a in &mut p.assets {
        ensure!(
            ids.insert(a.id) && a.duration.is_finite() && a.duration > 0.,
            "素材情報が不正です"
        );
        if a.path.is_relative() {
            a.path = path
                .parent()
                .context("プロジェクトの場所が不明です")?
                .join(&a.path);
        }
    }
    for c in &p.clips {
        ensure!(
            c.valid_source(&p)
                && ids.insert(c.id)
                && c.lane < 1024
                && c.start >= 0
                && c.len > 0
                && c.start.checked_add(c.len).is_some()
                && c.offset >= 0
                && c.offset.checked_add(c.len).is_some(),
            "クリップ情報が不正です"
        );
    }
    let max_id = ids
        .iter()
        .copied()
        .chain(p.clips.iter().filter_map(|c| c.link))
        .max()
        .unwrap_or(0);
    ensure!(
        p.next > max_id && p.next < u64::MAX / 2,
        "項目 ID が不正です"
    );
    Ok(p)
}
impl Clip {
    pub fn valid_source(&self, p: &Project) -> bool {
        self.title.is_some() || p.assets.iter().any(|a| a.id == self.asset)
    }
}
impl Editor {
    pub fn dirty(&self) -> bool {
        self.p != self.saved
    }
    pub fn save_project(&mut self, choose: bool) -> bool {
        let path = if choose { None } else { self.path.clone() }.or_else(|| {
            rfd::FileDialog::new()
                .add_filter("FastEdit", &["fastedit"])
                .set_file_name("名称未設定.fastedit")
                .save_file()
        });
        let Some(path) = path else {
            return false;
        };
        match save(&path, &self.p) {
            Ok(()) => {
                self.path = Some(path);
                self.saved = self.p.clone();
                self.status = "保存しました".into();
                true
            }
            Err(e) => {
                self.status = e.to_string();
                false
            }
        }
    }
    pub fn allow_discard(&mut self) -> bool {
        if !self.dirty() {
            return true;
        }
        use rfd::{MessageButtons, MessageDialog, MessageDialogResult};
        match MessageDialog::new()
            .set_title("FastEdit")
            .set_description("変更を保存しますか？")
            .set_buttons(MessageButtons::YesNoCancel)
            .show()
        {
            MessageDialogResult::Yes => self.save_project(false),
            MessageDialogResult::No => true,
            _ => false,
        }
    }
    pub fn open_project(&mut self, path: std::path::PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if !self.allow_discard() {
            return;
        }
        match load(&path) {
            Ok(p) => self.enter_project(p, Some(path)),
            Err(e) => self.status = e.to_string(),
        }
    }
    pub fn enter_project(&mut self, p: Project, path: Option<std::path::PathBuf>) {
        self.home = false;
        self.title_edit = None;
        self.active_lane = 0;
        self.return_to_editor = false;
        self.pending_media.clear();
        self.drag = None;
        self.trim_flash = None;
        self.canvas_drag = None;
        self.saved = p.clone();
        self.p = p;
        self.path = path;
        self.undo.clear();
        self.redo.clear();
        self.selected = None;
        self.texture = None;
        (self.inbox, self.imports) = crate::media::import_queue();
        self.importing = 0;
        self.status.clear();
        self.seek(0);
    }
    pub fn choose_project(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("FastEdit", &["fastedit"])
            .pick_file()
        {
            self.open_project(path);
        }
    }
    pub fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("ファイル", |ui| {
            if ui.button("新規").clicked() {
                ui.close_menu();
                self.preview = None;
                self.home = true;
                self.return_to_editor = true;
                self.new_size = [1920, 1080];
                self.status.clear();
            }
            if ui.button("プロジェクトを開く…").clicked() {
                ui.close_menu();
                self.choose_project();
            }
            if ui.button("別名で保存…").clicked() {
                ui.close_menu();
                self.save_project(true);
            }
        });
        if ui.button("保存").clicked() {
            self.save_project(false);
        }
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("名称未設定");
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "{name}{} — FastEdit",
                if self.dirty() { " *" } else { "" }
            )));
    }
    pub fn relink(&mut self, id: u64) {
        let Some(path) = rfd::FileDialog::new().pick_file() else {
            return;
        };
        let result = crate::media::probe(&path).and_then(|mut a| {
            let old = self
                .p
                .assets
                .iter()
                .find(|a| a.id == id)
                .context("素材がありません")?;
            ensure!(
                a.audio.is_some() == old.audio.is_some()
                    && a.video.is_some() == old.video.is_some()
                    && a.image == old.image,
                "同じ種類の素材を選択してください"
            );
            ensure!(
                a.image
                    || self
                        .p
                        .clips
                        .iter()
                        .filter(|c| c.asset == id)
                        .all(|c| c.offset + c.len <= self.p.rate.frames(a.duration)),
                "参照区間より短い素材です"
            );
            let before = self.p.clone();
            a.id = id;
            *self.p.assets.iter_mut().find(|a| a.id == id).unwrap() = a;
            self.record(before);
            Ok(())
        });
        if let Err(e) = result {
            self.status = e.to_string();
        }
    }
}
