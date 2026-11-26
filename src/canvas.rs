use crate::Editor;
use eframe::egui;

impl Editor {
    pub fn canvas_settings(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(format!("画面サイズ  {} × {}", self.p.width, self.p.height))
            .default_open(true)
            .show(ui, |ui| {
                let before = self.p.clone();
                egui::ComboBox::from_id_salt("size")
                    .selected_text("サイズを選択…")
                    .show_ui(ui, |ui| {
                        for (label, w, h) in [
                            ("Full HD  •  1920 × 1080", 1920, 1080),
                            ("4K UHD  •  3840 × 2160", 3840, 2160),
                            ("縦型  •  1080 × 1920", 1080, 1920),
                            ("正方形  •  1080 × 1080", 1080, 1080),
                        ] {
                            if ui
                                .selectable_label(self.p.width == w && self.p.height == h, label)
                                .clicked()
                            {
                                self.p.width = w;
                                self.p.height = h;
                            }
                        }
                    });
                ui.horizontal_wrapped(|ui| {
                    ui.label("幅");
                    let w = ui
                        .add(egui::DragValue::new(&mut self.p.width).range(2..=16384))
                        .changed();
                    ui.label("高さ");
                    let h = ui
                        .add(egui::DragValue::new(&mut self.p.height).range(2..=16384))
                        .changed();
                    if self.lock_ratio {
                        let ratio = before.width as f64 / before.height as f64;
                        if w {
                            self.p.height =
                                (self.p.width as f64 / ratio).round().clamp(2., 16384.) as u32;
                        } else if h {
                            self.p.width =
                                (self.p.height as f64 * ratio).round().clamp(2., 16384.) as u32;
                        }
                    }
                });
                ui.checkbox(&mut self.lock_ratio, "縦横比を固定");
                if !self.p.width.is_multiple_of(2) || !self.p.height.is_multiple_of(2) {
                    let w = self.p.width.next_multiple_of(2);
                    let h = self.p.height.next_multiple_of(2);
                    if ui.button(format!("MP4 用に {w} × {h} に調整")).clicked() {
                        self.p.width = w;
                        self.p.height = h;
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("背景色");
                    ui.color_edit_button_srgb(&mut self.p.background);
                });
                self.record(before);
            });
    }
}
