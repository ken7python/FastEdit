use super::*;
use cosmic_text::{
    Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap,
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{LazyLock, Mutex},
};

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Layout {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub scale_y: Option<f32>,
    pub fill: bool,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            scale: 1.,
            scale_y: None,
            fill: false,
        }
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Title {
    pub body: String,
    pub font: String,
    pub size: f32,
    pub rgb: [u8; 3],
    pub align: u8,
}
impl Title {
    pub fn dimensions(&self, p: &Project) -> (u32, u32) {
        (
            (p.width as f32 * 0.9).ceil() as u32,
            (self.body.split('\n').count() as f32 * self.size * 1.3)
                .ceil()
                .clamp(2., p.height as f32) as u32,
        )
    }
    pub fn raster(&self, p: &Project, path: &Path) -> anyhow::Result<()> {
        static SYSTEM: LazyLock<Mutex<FontSystem>> =
            LazyLock::new(|| Mutex::new(FontSystem::new()));
        let mut fs = SYSTEM
            .lock()
            .map_err(|_| anyhow::anyhow!("フォントの初期化に失敗しました"))?;
        let (w, h) = self.dimensions(p);
        let mut buf = Buffer::new(&mut fs, Metrics::new(self.size, self.size * 1.3));
        buf.set_size(&mut fs, Some(w as f32), Some(h as f32));
        buf.set_wrap(&mut fs, Wrap::None);
        buf.set_text(
            &mut fs,
            &self.body,
            Attrs::new().family(Family::Name(&self.font)),
            Shaping::Advanced,
        );
        for line in &mut buf.lines {
            line.set_align(Some(match self.align {
                0 => Align::Left,
                2 => Align::Right,
                _ => Align::Center,
            }));
        }
        buf.shape_until_scroll(&mut fs, false);
        let mut bitmap = image::RgbaImage::new(w, h);
        let [r, g, b] = self.rgb;
        buf.draw(
            &mut fs,
            &mut SwashCache::new(),
            Color::rgb(r, g, b),
            |x, y, _, _, c| {
                if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                    bitmap.put_pixel(
                        x as u32,
                        y as u32,
                        image::Rgba([c.r(), c.g(), c.b(), c.a()]),
                    );
                }
            },
        );
        bitmap.save(path)?;
        Ok(())
    }
}
impl Clip {
    fn preview_bounds(&self, p: &Project, canvas: egui::Rect) -> egui::Rect {
        let (w, h) = self.visual_size(p);
        let size = egui::vec2(
            w / p.width as f32 * canvas.width(),
            h / p.height as f32 * canvas.height(),
        );
        let center = canvas.min
            + egui::vec2(
                self.layout.x * canvas.width(),
                self.layout.y * canvas.height(),
            );
        egui::Rect::from_center_size(center, size)
    }
    pub fn visual_size(&self, p: &Project) -> (f32, f32) {
        if let Some(t) = &self.title {
            let (w, h) = t.dimensions(p);
            return (
                w as f32 * self.layout.scale,
                h as f32 * self.layout.scale_y.unwrap_or(self.layout.scale),
            );
        }
        let Some(a) = p.assets.iter().find(|a| a.id == self.asset) else {
            return (1., 1.);
        };
        let sx = p.width as f32 / a.width.max(1) as f32;
        let sy = p.height as f32 / a.height.max(1) as f32;
        let factor = if self.layout.fill {
            sx.max(sy)
        } else {
            sx.min(sy)
        } * self.layout.scale;
        (a.width as f32 * factor, a.height as f32 * factor)
    }
}
impl Editor {
    pub fn add_title(&mut self) {
        let old = self.p.clone();
        let id = self.p.id();
        let lane = if self.active_lane == 0 {
            self.front_lane()
        } else {
            self.active_lane
        };
        self.active_lane = lane;
        let font = self
            .families
            .iter()
            .find(|s| {
                matches!(
                    s.as_str(),
                    "Hiragino Sans" | "Yu Gothic" | "Noto Sans CJK JP"
                )
            })
            .or(self.families.first())
            .cloned()
            .unwrap_or_default();
        self.p.clips.push(Clip {
            id,
            asset: 0,
            gain_db: 0.,
            muted: false,
            kind: Kind::Video,
            lane,
            start: self.cursor,
            len: self.p.rate.frames(3.).max(1),
            offset: 0,
            link: None,
            layout: Layout::default(),
            title: Some(Title {
                body: "テキスト".into(),
                font,
                size: (self.p.height as f32 * 0.07).max(12.),
                rgb: [255; 3],
                align: 1,
            }),
        });
        self.selected = Some(id);
        self.record(old);
    }
    pub fn front_lane(&self) -> u32 {
        self.p
            .clips
            .iter()
            .map(|c| c.lane)
            .max()
            .unwrap_or(0)
            .max(self.p.highest_video_track())
            .saturating_add(1)
            .min(1023)
    }
    pub fn layer_settings(&mut self, ui: &mut egui::Ui, id: u64) {
        let before = self.p.clone();
        let Some(c) = self
            .p
            .clips
            .iter_mut()
            .find(|c| c.id == id && c.kind == Kind::Video)
        else {
            return;
        };
        if let Some(t) = &mut c.title {
            ui.separator();
            ui.label("テキスト");
            ui.text_edit_multiline(&mut t.body);
            if !self.families.contains(&t.font) {
                ui.colored_label(Color32::YELLOW, "代替フォントを使用中");
            }
            egui::ComboBox::from_id_salt("family")
                .selected_text(&t.font)
                .show_ui(ui, |ui| {
                    for f in &self.families {
                        ui.selectable_value(&mut t.font, f.clone(), f);
                    }
                });
            ui.horizontal(|ui| {
                ui.label("文字サイズ");
                ui.add(egui::DragValue::new(&mut t.size).range(4.0..=1024.0));
                ui.color_edit_button_srgb(&mut t.rgb);
            });
            ui.horizontal(|ui| {
                for (i, label) in ["左揃え", "中央", "右揃え"].iter().enumerate() {
                    ui.selectable_value(&mut t.align, i as u8, *label);
                }
            });
        } else {
            ui.checkbox(&mut c.layout.fill, "画面いっぱいに配置");
        }
        ui.horizontal(|ui| {
            ui.label("横位置");
            ui.add(
                egui::DragValue::new(&mut c.layout.x)
                    .speed(0.005)
                    .range(-1.0..=2.0),
            );
            ui.label("縦位置");
            ui.add(
                egui::DragValue::new(&mut c.layout.y)
                    .speed(0.005)
                    .range(-1.0..=2.0),
            );
        });
        ui.horizontal(|ui| {
            let text = c.title.is_some();
            let old_y = c.layout.scale_y.unwrap_or(c.layout.scale);
            ui.label(if text { "横倍率" } else { "倍率" });
            let x = ui
                .add(
                    egui::DragValue::new(&mut c.layout.scale)
                        .speed(0.01)
                        .range(0.02..=4.),
                )
                .changed();
            if text {
                let mut y = old_y;
                ui.label("縦倍率");
                if ui
                    .add(egui::DragValue::new(&mut y).speed(0.01).range(0.02..=4.))
                    .changed()
                    || x
                {
                    c.layout.scale_y = Some(y);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("レーン");
            ui.add(egui::DragValue::new(&mut c.lane).range(0..=1023));
        });
        self.record(before);
    }
    pub fn overlay_handles(&mut self, ui: &mut egui::Ui, canvas: egui::Rect) {
        if self.title_edit.is_none()
            && ui.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            })
            && let Some(point) = ui
                .input(|i| i.pointer.interact_pos())
                .filter(|p| canvas.contains(*p))
            && let Some(hit) = self
                .p
                .clips
                .iter()
                .filter(|c| {
                    c.kind == Kind::Video
                        && c.start <= self.cursor
                        && self.cursor < c.end()
                        && c.preview_bounds(&self.p, canvas).contains(point)
                })
                .max_by_key(|c| c.lane)
            && hit.title.is_some()
        {
            self.edit_title(hit.id);
            return;
        }
        let Some(c) = self
            .selected
            .and_then(|id| self.p.clip(id))
            .filter(|c| c.kind == Kind::Video && c.start <= self.cursor && self.cursor < c.end())
            .cloned()
        else {
            return;
        };
        let bounds = c.preview_bounds(&self.p, canvas);
        let size = bounds.size();
        let painter = ui.painter_at(canvas);
        painter.rect_stroke(
            bounds,
            1.,
            egui::Stroke::new(1.0_f32, Color32::LIGHT_BLUE),
            egui::StrokeKind::Inside,
        );
        let corner =
            egui::Rect::from_center_size(bounds.max, egui::vec2(14., 14.)).intersect(canvas);
        painter.rect_filled(corner, 2., Color32::LIGHT_BLUE);
        for (rect, scale) in [(bounds.intersect(canvas), false), (corner, true)] {
            if !rect.is_positive() {
                continue;
            }
            let r = ui.interact(
                rect,
                ui.id().with(("transform", scale)),
                egui::Sense::drag(),
            );
            if r.drag_started() {
                self.canvas_drag = Some(self.p.clone());
            }
            if r.dragged()
                && let Some(old) = self.canvas_drag.as_ref().and_then(|p| p.clip(c.id))
            {
                let d = ui.input(|i| {
                    i.pointer
                        .press_origin()
                        .zip(i.pointer.interact_pos())
                        .map(|(a, b)| b - a)
                        .unwrap_or_default()
                });
                let clip = self.p.clips.iter_mut().find(|x| x.id == c.id).unwrap();
                if scale {
                    if clip.title.is_some() {
                        let sy = c.layout.scale_y.unwrap_or(c.layout.scale);
                        clip.layout.scale_y = Some(
                            (old.layout.scale_y.unwrap_or(old.layout.scale)
                                + 2. * d.y / (size.y / sy).max(1.))
                            .clamp(0.02, 4.),
                        );
                    }
                    clip.layout.scale = (old.layout.scale
                        + 2. * d.x / (size.x / c.layout.scale).max(1.))
                    .clamp(0.02, 4.);
                } else {
                    clip.layout.x = (old.layout.x + d.x / canvas.width()).clamp(-1., 2.);
                    clip.layout.y = (old.layout.y + d.y / canvas.height()).clamp(-1., 2.);
                }
                self.invalidate();
            }
            if r.drag_stopped()
                && let Some(old) = self.canvas_drag.take()
            {
                self.record(old);
            }
        }
    }
}

pub struct TitleEdit {
    id: u64,
    body: String,
    focus: bool,
}
impl Editor {
    pub fn edit_title(&mut self, id: u64) {
        if let Some(t) = self.p.clip(id).and_then(|c| c.title.as_ref()) {
            self.title_edit = Some(TitleEdit {
                id,
                body: t.body.clone(),
                focus: true,
            });
            self.selected = Some(id);
            self.invalidate();
        }
    }
    pub fn title_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.title_edit.as_mut() else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let dialog = egui::Modal::new("title-editor".into()).show(ctx, |ui| {
            ui.set_min_width(320.);
            ui.heading("テキストを編集");
            let field = ui.add(
                egui::TextEdit::multiline(&mut draft.body)
                    .desired_width(320.)
                    .desired_rows(4),
            );
            if draft.focus {
                field.request_focus();
                draft.focus = false;
            }
            ui.horizontal(|ui| {
                apply = ui.button("適用").clicked();
                cancel = ui.button("キャンセル").clicked();
            });
        });
        if apply || cancel || dialog.should_close() {
            let change = self.title_edit.take().unwrap();
            if apply {
                let before = self.p.clone();
                if let Some(t) = self
                    .p
                    .clips
                    .iter_mut()
                    .find(|c| c.id == change.id)
                    .and_then(|c| c.title.as_mut())
                {
                    t.body = change.body;
                }
                self.record(before);
            }
        }
    }
}
