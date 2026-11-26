use super::*;
use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

impl Editor {
    pub fn record(&mut self, before: Project) {
        if before == self.p {
            return;
        }
        if self.undo.len() >= 100 {
            self.undo.remove(0);
        }
        self.undo.push(before);
        self.redo.clear();
        self.cursor = self.cursor.min(self.p.end());
        self.invalidate();
    }
    pub fn history(&mut self, back: bool) {
        let (take, store) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        if let Some(p) = take.pop() {
            store.push(std::mem::replace(&mut self.p, p));
            self.seek(self.cursor);
        }
    }
    fn action(&mut self, mode: u8) {
        if let Some(id) = self.selected {
            let old = self.p.clone();
            if mode == 0 {
                self.p.split(id, self.cursor)
            } else {
                self.p.delete(id, mode == 2);
            }
            self.record(old);
        }
    }
    pub fn tracks(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (label, mode) in [("分割", 0), ("削除", 1), ("詰めて削除（全トラック）", 2)]
            {
                let enabled = self
                    .selected
                    .and_then(|id| self.p.clip(id))
                    .is_some_and(|c| match mode {
                        0 => self.cursor > c.start && self.cursor < c.end(),
                        2 => c.lane == 0 && c.kind == Kind::Video,
                        _ => true,
                    });
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    self.action(mode);
                }
            }
            ui.separator();
            ui.label("ズーム");
            ui.add(
                egui::Slider::new(&mut self.zoom, 1.0..=16.0)
                    .logarithmic(true)
                    .show_value(false),
            );
        });
        let highest = self.p.clips.iter().map(|c| c.lane).max().unwrap_or(0);
        let rows: Vec<_> = [Kind::Video, Kind::Audio]
            .into_iter()
            .flat_map(|k| (0..=highest).rev().map(move |l| (k.clone(), l)))
            .filter(|(k, l)| *l == 0 || self.p.clips.iter().any(|c| c.kind == *k && c.lane == *l))
            .collect();
        egui::ScrollArea::both().id_salt("tracks").show(ui, |ui| {
            let width = (ui.available_width() - 6.).max(300.) * self.zoom;
            let height = 24. + rows.len() as f32 * 31.;
            let (rect, area) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
            let pixels = (width - 2.) / self.p.end().max(1) as f32;
            let x = |frame: i64| rect.left() + frame as f32 * pixels;
            let to_frame = |px: f32| ((px - rect.left()) / pixels).round() as i64;
            let painter = ui.painter_at(rect);
            for n in 0..=10 {
                let f = self.p.end() * n / 10;
                painter.text(
                    pos2(x(f) + 2., rect.top() + 2.),
                    Align2::LEFT_TOP,
                    format!("{:.1}s", self.p.rate.seconds(f)),
                    FontId::monospace(10.),
                    Color32::GRAY,
                );
            }
            if (area.clicked() || area.dragged())
                && let Some(p) = area.interact_pointer_pos()
            {
                self.seek(to_frame(p.x));
            }
            for (row, (kind, lane)) in rows.iter().enumerate() {
                let y = rect.top() + 21. + row as f32 * 31.;
                painter.rect_filled(
                    Rect::from_min_size(pos2(rect.left(), y), vec2(width, 28.)),
                    3.,
                    Color32::from_gray(31),
                );
                for c in self
                    .p
                    .clips
                    .clone()
                    .into_iter()
                    .filter(|c| c.kind == *kind && c.lane == *lane)
                {
                    let block = Rect::from_min_max(
                        pos2(x(c.start), y),
                        pos2(x(c.end()).max(x(c.start) + 3.), y + 27.),
                    );
                    let resp = ui.interact(block, ui.id().with(c.id), Sense::click_and_drag());
                    let color = if c.kind == Kind::Audio {
                        Color32::from_rgb(34, 91, 81)
                    } else {
                        Color32::from_rgb(59, 77, 127)
                    };
                    painter.rect_filled(block, 4., color);
                    if self.selected == Some(c.id) {
                        painter.rect_stroke(
                            block,
                            4.,
                            Stroke::new(2., Color32::LIGHT_BLUE),
                            StrokeKind::Inside,
                        );
                    }
                    let name = self
                        .p
                        .assets
                        .iter()
                        .find(|a| a.id == c.asset)
                        .and_then(|a| a.path.file_name())
                        .map(|s| s.to_string_lossy())
                        .unwrap_or("テキスト".into());
                    let label = if c.kind == Kind::Audio {
                        "音声"
                    } else {
                        "映像"
                    };
                    painter
                        .with_clip_rect(block.intersect(ui.clip_rect()))
                        .text(
                            block.left_center() + vec2(6., 0.),
                            Align2::LEFT_CENTER,
                            format!("{label}  {name}"),
                            FontId::proportional(12.),
                            Color32::WHITE,
                        );
                    if resp.clicked() {
                        self.selected = Some(c.id);
                        if let Some(p) = resp.interact_pointer_pos() {
                            self.seek(to_frame(p.x));
                        }
                    }
                    if resp.drag_started() {
                        self.selected = Some(c.id);
                        let px = ui
                            .input(|i| i.pointer.press_origin())
                            .map_or(block.center().x, |p| p.x);
                        let edge = 6_f32.min(block.width() / 3.);
                        let handle = if px - block.left() < edge {
                            Handle::Left
                        } else if block.right() - px < edge {
                            Handle::Right
                        } else {
                            Handle::Body
                        };
                        self.drag = Some((self.p.clone(), handle, pixels));
                    }
                    if resp.dragged()
                        && let Some((p, handle, scale)) = &self.drag
                    {
                        self.p = p.clone();
                        self.p.adjust(
                            c.id,
                            *handle,
                            (ui.input(|i| {
                                i.pointer
                                    .press_origin()
                                    .zip(i.pointer.interact_pos())
                                    .map_or(0., |(a, b)| b.x - a.x)
                            }) / scale)
                                .round() as i64,
                        );
                        self.invalidate();
                    }
                    if resp.drag_stopped()
                        && let Some((before, _, _)) = self.drag.take()
                    {
                        self.record(before);
                    }
                }
            }
            painter.line_segment(
                [
                    pos2(x(self.cursor), rect.top()),
                    pos2(x(self.cursor), rect.bottom()),
                ],
                Stroke::new(1.5, Color32::from_rgb(255, 179, 97)),
            );
        });
    }
}
