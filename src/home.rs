use crate::{Editor, Project};
use eframe::egui::{self, Color32, RichText};

pub const FORMATS: [(&str, u32, u32); 4] = [
    ("Full HD", 1920, 1080),
    ("4K UHD", 3840, 2160),
    ("縦型", 1080, 1920),
    ("正方形", 1080, 1080),
];

impl Editor {
    pub fn project_home(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let width = ui.available_width().min(720.);
            let rect =
                egui::Rect::from_center_size(ui.max_rect().center(), egui::vec2(width, 370.));
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                ui.label(RichText::new("FastEdit").size(36.).strong());
                ui.add_space(18.);
                ui.heading("新規プロジェクト");
                ui.label("動画の画面サイズを選択");
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    for (label, w, h) in FORMATS {
                        let selected = self.new_size == [w, h];
                        let (card, response) =
                            ui.allocate_exact_size(egui::vec2(154., 98.), egui::Sense::click());
                        let painter = ui.painter();
                        painter.rect_filled(
                            card,
                            8.,
                            Color32::from_gray(if response.hovered() { 48 } else { 35 }),
                        );
                        let accent = if selected {
                            Color32::from_rgb(30, 215, 96)
                        } else {
                            Color32::from_gray(179)
                        };
                        if selected {
                            painter.rect_stroke(
                                card,
                                8.,
                                egui::Stroke::new(2., accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let ratio = w as f32 / h as f32;
                        let icon = egui::Rect::from_center_size(
                            card.left_top() + egui::vec2(30., 27.),
                            egui::vec2(32. * ratio.min(1.), 32. / ratio.max(1.)),
                        );
                        painter.rect_stroke(
                            icon,
                            2.,
                            egui::Stroke::new(2., accent),
                            egui::StrokeKind::Inside,
                        );
                        painter.text(
                            card.left_top() + egui::vec2(14., 51.),
                            egui::Align2::LEFT_TOP,
                            label,
                            egui::FontId::proportional(17.),
                            Color32::WHITE,
                        );
                        painter.text(
                            card.left_top() + egui::vec2(14., 74.),
                            egui::Align2::LEFT_TOP,
                            format!("{w} × {h}"),
                            egui::FontId::proportional(12.),
                            Color32::LIGHT_GRAY,
                        );
                        if response.clicked() {
                            self.new_size = [w, h];
                        }
                    }
                });
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    ui.label("幅");
                    ui.add(egui::DragValue::new(&mut self.new_size[0]).range(2..=16384));
                    ui.label("× 高さ");
                    ui.add(egui::DragValue::new(&mut self.new_size[1]).range(2..=16384));
                    ui.label("px");
                });
                ui.add_space(14.);
                ui.horizontal(|ui| {
                    if ui.button("プロジェクトを作成").clicked() && self.allow_discard() {
                        let project = Project {
                            width: self.new_size[0],
                            height: self.new_size[1],
                            ..Default::default()
                        };
                        self.enter_project(project, None);
                    }
                    if ui.button("プロジェクトを開く…").clicked() {
                        self.choose_project();
                    }
                    if self.return_to_editor && ui.button("編集に戻る").clicked() {
                        self.home = false;
                    }
                });
                if !self.status.is_empty() {
                    ui.label(&self.status);
                }
            });
        });
    }
}
