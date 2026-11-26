#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod asset_preview;
mod canvas;
mod home;
mod layers;
mod media;
mod model;
mod playback;
mod project_io;
mod render;
mod timeline;
use eframe::egui::{self, Color32, RichText};
use model::*;
use std::{
    path::PathBuf,
    sync::{atomic::Ordering, mpsc},
    time::{Duration, Instant},
};

struct Editor {
    p: Project,
    home: bool,
    new_size: [u32; 2],
    return_to_editor: bool,
    saved: Project,
    path: Option<PathBuf>,
    selected: Option<u64>,
    cache: asset_preview::Cache,
    families: Vec<String>,
    canvas_drag: Option<Project>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    zoom: f32,
    active_lane: u32,
    drag: Option<(Project, Handle, f32)>,
    cursor: i64,
    preview: Option<playback::Preview>,
    device: Option<playback::Device>,
    texture: Option<egui::TextureHandle>,
    refresh: Option<Instant>,
    status: String,
    importing: usize,
    imports: mpsc::Receiver<Result<Asset, String>>,
    inbox: mpsc::Sender<PathBuf>,
    export: Option<(render::Cancel, mpsc::Receiver<render::ExportEvent>)>,
    progress: f32,
    close_approved: bool,
    lock_ratio: bool,
}
fn fonts(ctx: &egui::Context) -> Vec<String> {
    let mut db = cosmic_text::fontdb::Database::new();
    db.load_system_fonts();
    for name in [
        "Hiragino Sans",
        "Yu Gothic",
        "Meiryo",
        "Noto Sans CJK JP",
        "Noto Sans JP",
    ] {
        if let Some(id) = db.query(&cosmic_text::fontdb::Query {
            families: &[cosmic_text::fontdb::Family::Name(name)],
            ..Default::default()
        }) && let Some((bytes, index)) = db.with_face_data(id, |b, i| (b.to_vec(), i))
        {
            let mut defs = egui::FontDefinitions::default();
            let mut font = egui::FontData::from_owned(bytes);
            font.index = index;
            defs.font_data
                .insert("jp".into(), std::sync::Arc::new(font));
            defs.families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .push("jp".into());
            ctx.set_fonts(defs);
            break;
        }
    }
    let mut names: Vec<_> = db
        .faces()
        .filter_map(|f| f.families.first().map(|(s, _)| s.clone()))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}
impl Editor {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let families = fonts(&cc.egui_ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(29, 32, 39);
        visuals.selection.bg_fill = Color32::from_rgb(47, 95, 119);
        cc.egui_ctx.set_visuals(visuals);
        cc.egui_ctx.style_mut(|s| {
            s.spacing.button_padding = egui::vec2(8., 4.);
            s.spacing.item_spacing = egui::vec2(6., 5.);
        });
        let (inbox, imports) = media::import_queue();
        let mut app = Self {
            p: Project::default(),
            home: true,
            new_size: [1920, 1080],
            return_to_editor: false,
            saved: Project::default(),
            path: None,
            selected: None,
            cache: asset_preview::Cache::new(),
            families,
            canvas_drag: None,
            undo: vec![],
            redo: vec![],
            zoom: 1.,
            active_lane: 0,
            drag: None,
            cursor: 0,
            preview: None,
            device: playback::Device::open(),
            texture: None,
            refresh: None,
            status: String::new(),
            importing: 0,
            imports,
            inbox,
            export: None,
            progress: 0.0,
            close_approved: false,
            lock_ratio: false,
        };
        for path in std::env::args_os().skip(1) {
            app.import(path.into());
        }
        app
    }
    fn import(&mut self, path: PathBuf) {
        if path.extension().is_some_and(|x| x == "fastedit") {
            self.open_project(path);
            return;
        }
        self.home = false;
        if self.inbox.send(path).is_ok() {
            self.importing += 1;
        }
    }
    fn picked(&mut self) {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            for path in paths {
                self.import(path);
            }
        }
    }
    fn invalidate(&mut self) {
        self.preview = None;
        self.refresh = Some(Instant::now() + Duration::from_millis(70));
    }
    fn seek(&mut self, at: i64) {
        self.cursor = at.clamp(0, self.p.end());
        self.invalidate();
    }
    fn toggle(&mut self) {
        if self.preview.as_ref().is_some_and(|p| p.playing) {
            self.invalidate();
            return;
        }
        if self.p.end() == 0 {
            return;
        }
        if self.cursor >= self.p.end() {
            self.cursor = 0;
        }
        self.refresh = None;
        self.preview = Some(playback::Preview::new(
            self.p.clone(),
            self.cursor,
            true,
            self.device.as_ref(),
        ));
    }
    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.imports.try_recv() {
            self.importing = self.importing.saturating_sub(1);
            match msg {
                Ok(mut a) => {
                    let before = self.p.clone();
                    a.id = self.p.id();
                    if self.p.clips.is_empty() && a.video.is_some() && !a.image {
                        self.p.rate = a.rate;
                    }
                    let start = self.p.end();
                    let first = self.p.assets.is_empty();
                    if first {
                        self.selected = Some(self.p.place(&a, start, self.active_lane));
                    }
                    self.p.assets.push(a);
                    self.record(before);
                    if first {
                        self.seek(start);
                    }
                }
                Err(s) => self.status = s,
            }
        }
        if self.refresh.is_some_and(|t| t <= Instant::now()) {
            self.refresh = None;
            if self.p.end() > 0 {
                self.preview = Some(playback::Preview::new(
                    self.p.clone(),
                    self.cursor.min(self.p.end() - 1),
                    false,
                    None,
                ));
            } else {
                self.texture = None;
            }
        }
        if let Some(p) = self.preview.as_mut() {
            if let Ok(e) = p.errors.try_recv() {
                self.status = e;
                self.preview = None;
            } else {
                let target = p.from + self.p.rate.frames(p.seconds());
                while p.displayed < target {
                    match p.frames.try_recv() {
                        Ok(img) => {
                            p.displayed += 1;
                            p.started();
                            self.texture =
                                Some(ctx.load_texture("preview", img, Default::default()));
                        }
                        Err(_) => break,
                    }
                }
                if p.playing {
                    self.cursor = target.min(self.p.end());
                    if self.cursor >= self.p.end() - 1 && p.displayed >= self.p.end() - 1 {
                        self.cursor = self.p.end();
                        self.invalidate();
                    }
                }
            }
        }
        if let Some((_, rx)) = &self.export {
            while let Ok(e) = rx.try_recv() {
                match e {
                    render::ExportEvent::Progress(n) => self.progress = n,
                    render::ExportEvent::Done(r) => {
                        self.status = r
                            .map(|_| "書き出しました".into())
                            .unwrap_or_else(|e| e.to_string());
                        self.export = None;
                        break;
                    }
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(
            if self.preview.as_ref().is_some_and(|p| p.playing) {
                12
            } else {
                80
            },
        ));
    }
    fn export_dialog(&mut self) {
        if !self.p.width.is_multiple_of(2) || !self.p.height.is_multiple_of(2) {
            self.status = "画面サイズ設定で幅と高さを偶数に調整してください".into();
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title(format!(
                "動画を書き出す（{} × {}）",
                self.p.width, self.p.height
            ))
            .add_filter("MP4", &["mp4"])
            .set_file_name("FastEdit.mp4")
            .save_file()
        {
            self.progress = 0.;
            self.export = Some(render::export(self.p.clone(), &path));
        }
    }
    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("FastEdit");
            self.file_menu(ui);
            if ui
                .add_enabled(!self.undo.is_empty(), egui::Button::new("元に戻す"))
                .clicked()
            {
                self.history(true);
            }
            if ui
                .add_enabled(!self.redo.is_empty(), egui::Button::new("やり直す"))
                .clicked()
            {
                self.history(false);
            }
            ui.separator();
            if ui.button("素材を追加").clicked() {
                self.picked();
            }
            if ui
                .add_enabled(
                    self.p.end() > 0 && self.export.is_none(),
                    egui::Button::new("動画を書き出す"),
                )
                .clicked()
            {
                self.export_dialog();
            }
            if let Some((flag, _)) = &self.export {
                ui.add(egui::ProgressBar::new(self.progress).desired_width(90.));
                if ui.button("中止").clicked() {
                    flag.store(true, Ordering::Relaxed);
                }
            }
        });
    }
    fn library(&mut self, ui: &mut egui::Ui) {
        ui.heading("素材");
        if ui.button("テキストを追加").clicked() {
            self.add_title();
        }
        ui.separator();
        for a in self.p.assets.clone() {
            if let Some(t) = self
                .cache
                .entries
                .get(&a.path)
                .and_then(|d| d.thumbnail.as_ref())
            {
                ui.add(egui::Image::new(t).max_width(ui.available_width().min(144.)));
            }
            let name = a.path.file_name().unwrap_or_default().to_string_lossy();
            ui.label(RichText::new(name.as_ref()).strong());
            ui.small(format!("{:.1} 秒", a.duration));
            if !a.path.exists() && ui.button("素材を再指定…").clicked() {
                self.relink(a.id);
            }
            if a.video.is_some()
                && ui
                    .small_button(format!("V{} に配置", self.active_lane + 1))
                    .on_hover_text("選択中のトラックの再生位置に配置")
                    .clicked()
            {
                let before = self.p.clone();
                self.selected = Some(self.p.place(&a, self.cursor, self.active_lane));
                self.record(before);
            }
            if a.video.is_some() && ui.small_button("上に重ねる").clicked() {
                let before = self.p.clone();
                let lane = self.front_lane();
                self.active_lane = lane;
                let id = self.p.place(&a, self.cursor, lane);
                if let Some(c) = self.p.clips.iter_mut().find(|c| c.id == id) {
                    c.layout.scale = 0.45;
                }
                self.selected = Some(id);
                self.record(before);
            }
            if a.audio.is_some() && ui.small_button("音声を追加").clicked() {
                let before = self.p.clone();
                let mut audio = a.clone();
                audio.video = None;
                let lane = self.front_lane();
                self.selected = Some(self.p.place(&audio, self.cursor, lane));
                self.record(before);
            }
            if ui.small_button("末尾につなぐ").clicked() {
                let before = self.p.clone();
                self.selected = Some(self.p.place(&a, self.p.end(), 0));
                self.record(before);
                self.invalidate();
            }
            ui.separator();
        }
    }
    fn timeline(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("再生 / 停止").clicked() {
                self.toggle();
            }
            if ui.button("◀").clicked() {
                self.seek(self.cursor - 1);
            }
            if ui.button("▶").clicked() {
                self.seek(self.cursor + 1);
            }
            ui.monospace(format!(
                "{:.2} / {:.2} 秒",
                self.p.rate.seconds(self.cursor),
                self.p.rate.seconds(self.p.end())
            ));
        });
        let mut pos = self.cursor;
        if ui
            .add(egui::Slider::new(&mut pos, 0..=self.p.end()).text("再生位置"))
            .changed()
        {
            self.seek(pos);
        }
        if let Some(p) = &self.preview
            && p.playing
        {
            let level = f32::from_bits(p.peak.load(Ordering::Relaxed));
            ui.add(
                egui::ProgressBar::new(level.min(1.))
                    .desired_width(160.)
                    .text(format!("ピーク {:.0} dB", 20. * level.max(0.0001).log10())),
            );
        }
        self.tracks(ui);
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.heading("設定");
        self.canvas_settings(ui);
        if let Some(c) = self.selected.and_then(|id| self.p.clip(id)).cloned() {
            ui.separator();
            self.layer_settings(ui, c.id);
            if let Some(group) = c.link
                && self
                    .p
                    .clips
                    .iter()
                    .filter(|x| x.link == Some(group))
                    .count()
                    > 1
                && ui.button("音声を切り離す").clicked()
            {
                let before = self.p.clone();
                for item in &mut self.p.clips {
                    if item.link == Some(group) {
                        item.link = None;
                    }
                }
                self.record(before);
            }
            let audio_id = if c.kind == Kind::Audio {
                Some(c.id)
            } else {
                c.link.and_then(|link| {
                    self.p
                        .clips
                        .iter()
                        .find(|x| x.link == Some(link) && x.kind == Kind::Audio)
                        .map(|x| x.id)
                })
            };
            if let Some(id) = audio_id {
                let before = self.p.clone();
                if let Some(audio) = self.p.clips.iter_mut().find(|x| x.id == id) {
                    ui.checkbox(&mut audio.muted, "ミュート");
                    ui.add(egui::Slider::new(&mut audio.gain_db, -60.0..=24.0).text("音量 dB"));
                }
                self.record(before);
            }
            ui.label("クリップ（フレーム）");
            for (label, value, handle) in [
                ("開始", c.start, Handle::Body),
                ("長さ", c.len, Handle::Right),
            ] {
                let mut n = value;
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui.add(egui::DragValue::new(&mut n)).changed() {
                        let old = self.p.clone();
                        self.p.adjust(c.id, handle, n - value);
                        self.record(old);
                    }
                });
            }
        }
    }
    fn canvas(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(avail, 6., Color32::from_rgb(15, 17, 21));
        if let Some(t) = &self.texture {
            let size = t.size_vec2();
            let scale = (avail.width() / size.x).min(avail.height() / size.y);
            let rect = egui::Rect::from_center_size(avail.center(), size * scale);
            ui.put(rect, egui::Image::new(t).fit_to_exact_size(rect.size()));
            self.overlay_handles(ui, rect);
        } else {
            let rect = egui::Rect::from_center_size(avail.center(), egui::vec2(200., 36.));
            if ui
                .put(rect, egui::Button::new("動画・画像・音声を追加"))
                .clicked()
            {
                self.picked();
            }
        }
    }
}
impl eframe::App for Editor {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll(ctx);
        self.cache.sync(ctx, &self.p.assets);
        self.active_lane = self.active_lane.min(self.p.highest_video_track());
        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.close_approved {
                self.close_approved = self.allow_discard();
            }
            if !self.close_approved || self.export.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            if self.close_approved
                && let Some((flag, _)) = &self.export
            {
                flag.store(true, Ordering::Relaxed);
            }
        } else if self.close_approved && self.export.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        for path in ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        }) {
            self.import(path);
        }
        if self.home {
            self.project_home(ctx);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save_project(false);
        }
        if !ctx.wants_keyboard_input() {
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                )
            }) {
                self.history(false);
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                self.history(true);
            }
            if ctx
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            {
                self.action(1);
            }
        }
        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.toggle();
        }
        egui::TopBottomPanel::top("top").show(ctx, |ui| self.header(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(if self.importing > 0 {
                "読み込み中…"
            } else {
                &self.status
            });
        });
        egui::TopBottomPanel::bottom("timeline")
            .resizable(true)
            .default_height(210.)
            .show(ctx, |ui| self.timeline(ui));
        egui::SidePanel::left("library")
            .default_width(185.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.library(ui))
            });
        egui::SidePanel::right("inspector")
            .default_width(240.)
            .min_width(204.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.inspector(ui))
            });
        egui::CentralPanel::default().show(ctx, |ui| self.canvas(ui));
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        if let Some((flag, _)) = &self.export {
            flag.store(true, Ordering::Relaxed);
        }
    }
}
fn main() -> eframe::Result {
    eframe::run_native(
        "FastEdit",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_icon(
                    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                        .expect("bundled app icon"),
                )
                .with_inner_size([1220., 780.])
                .with_min_inner_size([900., 620.]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(Editor::new(cc)))),
    )
}
