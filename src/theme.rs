use eframe::egui::{self, Color32, RichText, Stroke};

pub const GREEN: Color32 = Color32::from_rgb(30, 215, 96);
pub const SILVER: Color32 = Color32::from_gray(179);
pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_gray(18);
    v.window_fill = Color32::from_gray(31);
    v.extreme_bg_color = Color32::from_gray(13);
    v.faint_bg_color = Color32::from_gray(24);
    v.window_corner_radius = 12.into();
    v.window_stroke = Stroke::NONE;
    v.selection.bg_fill = Color32::from_rgb(25, 91, 48);
    v.selection.stroke = Stroke::new(1_f32, GREEN);
    for (widget, fill, ink) in [
        (&mut v.widgets.inactive, 34, 230),
        (&mut v.widgets.hovered, 52, 255),
        (&mut v.widgets.active, 65, 255),
        (&mut v.widgets.open, 47, 255),
    ] {
        widget.bg_fill = Color32::from_gray(fill);
        widget.weak_bg_fill = widget.bg_fill;
        widget.bg_stroke = Stroke::NONE;
        widget.fg_stroke = Stroke::new(1.3_f32, Color32::from_gray(ink));
        widget.corner_radius = 16.into();
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1_f32, SILVER);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, Color32::from_gray(38));
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.button_padding = egui::vec2(11., 5.);
        s.spacing.item_spacing = egui::vec2(7., 6.);
        s.spacing.interact_size.y = 26.;
        s.visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(128),
        };
    });
}
pub fn primary(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).strong().color(Color32::BLACK))
        .fill(GREEN)
        .corner_radius(24.)
}
pub fn brand(ui: &mut egui::Ui, size: f32) {
    ui.label(
        RichText::new("FastEdit")
            .size(size)
            .strong()
            .color(Color32::WHITE),
    );
}
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_gray(24))
        .corner_radius(8.)
        .inner_margin(12)
}
pub fn transport(ui: &mut egui::Ui, playing: bool) -> bool {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(34., 34.), egui::Sense::click());
    let c = rect.center();
    let p = ui.painter();
    p.circle_filled(c, if r.hovered() { 17. } else { 16. }, GREEN);
    if playing {
        for dx in [-4., 3.] {
            p.rect_filled(
                egui::Rect::from_center_size(c + egui::vec2(dx, 0.), egui::vec2(3., 12.)),
                1.,
                Color32::BLACK,
            );
        }
    } else {
        p.add(egui::Shape::convex_polygon(
            vec![
                c + egui::vec2(-4., -7.),
                c + egui::vec2(7., 0.),
                c + egui::vec2(-4., 7.),
            ],
            Color32::BLACK,
            Stroke::NONE,
        ));
    }
    r.on_hover_text(if playing {
        "一時停止（Space）"
    } else {
        "再生（Space）"
    })
    .clicked()
}
