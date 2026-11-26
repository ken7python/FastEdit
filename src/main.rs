#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
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
    saved: Project,
    path: Option<PathBuf>,
    selected: Option<u64>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    zoom: f32,
    drag: Option<(Project, Handle, f32)>,
    cursor: i64,
    preview: Option<playback::Preview>,
    device: Option<playback::Device>,
    texture: Option<egui::TextureHandle>,
    refresh: Option<Instant>,
    status: String,
    importing: bool,
    imports: mpsc::Receiver<Result<Asset, String>>,
    inbox: mpsc::Sender<Result<Asset, String>>,
    export: Option<(render::Cancel, mpsc::Receiver<render::ExportEvent>)>,
    progress: f32,
}
fn fonts(ctx: &egui::Context) {
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
}
impl Editor {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let (inbox, imports) = mpsc::channel();
        let mut app = Self {
            p: Project::default(),
            saved: Project::default(),
            path: None,
            selected: None,
            undo: vec![],
            redo: vec![],
            zoom: 1.,
            drag: None,
            cursor: 0,
            preview: None,
            device: playback::Device::open(),
            texture: None,
            refresh: None,
            status: String::new(),
            importing: false,
            imports,
            inbox,
            export: None,
            progress: 0.0,
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
        let tx = self.inbox.clone();
        self.importing = true;
        std::thread::spawn(move || {
            let _ = tx.send(media::probe(&path).map_err(|e| format!("{} — {e}", path.display())));
        });
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
            self.importing = false;
            match msg {
                Ok(mut a) => {
                    let before = self.p.clone();
                    a.id = self.p.id();
                    if self.p.clips.is_empty() && a.video.is_some() {
                        self.p.width = a.width.max(2);
                        self.p.height = a.height.max(2);
                        if !a.image {
                            self.p.rate = a.rate;
                        }
                    }
                    let start = self.p.end();
                    self.selected = Some(self.p.place(&a, start, 0));
                    self.p.assets.push(a);
                    self.record(before);
                    self.seek(start);
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
        if let Some(path) = rfd::FileDialog::new()
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
        ui.separator();
        for a in self.p.assets.clone() {
            let name = a.path.file_name().unwrap_or_default().to_string_lossy();
            ui.label(RichText::new(name.as_ref()).strong());
            ui.small(format!("{:.1} 秒", a.duration));
            if !a.path.exists() && ui.button("素材を再指定…").clicked() {
                self.relink(a.id);
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
        self.tracks(ui);
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.heading("設定");
        ui.label(format!("{} × {}", self.p.width, self.p.height));
        if let Some(c) = self.selected.and_then(|id| self.p.clip(id)).cloned() {
            ui.separator();
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
        } else {
            ui.allocate_ui_at_rect(avail, |ui| {
                ui.centered_and_justified(|ui| {
                    if ui.button("動画・画像・音声を追加").clicked() {
                        self.picked();
                    }
                });
            });
        }
    }
}
impl eframe::App for Editor {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_discard() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save_project(false);
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
        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.toggle();
        }
        egui::TopBottomPanel::top("top").show(ctx, |ui| self.header(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(if self.importing {
                "読み込み中…"
            } else {
                &self.status
            });
        });
        egui::TopBottomPanel::bottom("timeline")
            .resizable(true)
            .default_height(180.)
            .show(ctx, |ui| self.timeline(ui));
        egui::SidePanel::left("library")
            .default_width(185.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.library(ui))
            });
        egui::SidePanel::right("inspector")
            .default_width(210.)
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
                .with_inner_size([1220., 780.])
                .with_min_inner_size([900., 620.]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(Editor::new(cc)))),
    )
}
