use eframe::egui::{Color32, CornerRadius, Frame, Margin, Stroke};

pub const APP_BG: Color32 = Color32::from_rgb(23, 24, 33);
pub const SIDEBAR_BG: Color32 = Color32::from_rgb(27, 28, 39);
pub const SURFACE: Color32 = Color32::from_rgb(29, 30, 41);
pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(34, 35, 47);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(41, 42, 56);
pub const BORDER: Color32 = Color32::from_rgb(43, 45, 58);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(241, 241, 245);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(139, 142, 159);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(98, 102, 121);
pub const BLUE: Color32 = Color32::from_rgb(86, 168, 232);
pub const GREEN: Color32 = Color32::from_rgb(110, 209, 139);
pub const ORANGE: Color32 = Color32::from_rgb(243, 164, 91);
pub const RED: Color32 = Color32::from_rgb(240, 108, 115);
pub const PURPLE: Color32 = Color32::from_rgb(185, 130, 232);

pub fn surface() -> Frame {
    Frame::NONE
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::same(14))
}
pub fn apply(ctx: &eframe::egui::Context) {
    let mut fonts = eframe::egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        eframe::egui::FontData::from_static(include_bytes!("../assets/fonts/InterVariable.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "JetBrainsMono".into(),
        eframe::egui::FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMonoVariable.ttf"
        ))
        .into(),
    );
    fonts
        .families
        .insert(eframe::egui::FontFamily::Proportional, vec!["Inter".into()]);
    fonts.families.insert(
        eframe::egui::FontFamily::Monospace,
        vec!["JetBrainsMono".into()],
    );
    ctx.set_fonts(fonts);
    let mut visuals = eframe::egui::Visuals::dark();
    visuals.dark_mode = true;
    visuals.panel_fill = APP_BG;
    visuals.window_fill = SURFACE;
    visuals.faint_bg_color = SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(18, 19, 27);
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.button_frame = true;
    visuals.interact_cursor = Some(eframe::egui::CursorIcon::PointingHand);
    visuals.selection = eframe::egui::style::Selection {
        bg_fill: SURFACE_HOVER,
        stroke: Stroke::new(1.0_f32, BLUE),
    };
    visuals.widgets.inactive.weak_bg_fill = SURFACE_ELEVATED;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_SECONDARY);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BLUE);
    visuals.widgets.active.weak_bg_fill = BLUE;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, BLUE);
    ctx.set_visuals(visuals);
}
