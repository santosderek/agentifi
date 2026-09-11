//! Single source of truth for colour, spacing, radii, and typography.
//!
//! Views never invent their own values. The visual language is a calm, dense,
//! dark operations console: neutral charcoal layers, one hairline border per
//! surface, semantic colour reserved for small status marks, and typography
//! (not colour) carrying the hierarchy.

use agentifi_domain::SessionStatus;
use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Frame, Margin, Rect, Stroke, TextStyle, Ui,
};

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

pub const APP_BG: Color32 = Color32::from_rgb(23, 24, 33);
pub const SIDEBAR_BG: Color32 = Color32::from_rgb(27, 28, 39);
pub const SURFACE: Color32 = Color32::from_rgb(29, 30, 41);
pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(34, 35, 47);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(41, 42, 56);
pub const SURFACE_SELECTED: Color32 = Color32::from_rgb(38, 41, 56);
pub const CODE_BG: Color32 = Color32::from_rgb(18, 19, 27);

pub const BORDER: Color32 = Color32::from_rgb(43, 45, 58);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(35, 37, 48);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(241, 241, 245);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(139, 142, 159);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(98, 102, 121);

pub const BLUE: Color32 = Color32::from_rgb(86, 168, 232);
pub const GREEN: Color32 = Color32::from_rgb(110, 209, 139);
pub const ORANGE: Color32 = Color32::from_rgb(243, 164, 91);
pub const RED: Color32 = Color32::from_rgb(240, 108, 115);
pub const PURPLE: Color32 = Color32::from_rgb(185, 130, 232);

// ---------------------------------------------------------------------------
// Spacing, radii, and sizing
// ---------------------------------------------------------------------------

pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 18.0;
pub const SPACE_XL: f32 = 28.0;
pub const SPACE_2XL: f32 = 44.0;

pub const RADIUS_CONTROL: u8 = 7;
pub const RADIUS_CARD: u8 = 10;

pub const ROW_HEIGHT: f32 = 64.0;
pub const RAIL_WIDTH: f32 = 214.0;
/// Reading width for the minimal Overview column.
pub const CONTENT_MAX_WIDTH: f32 = 760.0;

// ---------------------------------------------------------------------------
// Typography
// ---------------------------------------------------------------------------

#[must_use]
pub fn font_display() -> FontId {
    FontId::new(26.0, FontFamily::Monospace)
}
#[must_use]
pub fn font_title() -> FontId {
    FontId::new(17.0, FontFamily::Proportional)
}
#[must_use]
pub fn font_row_title() -> FontId {
    FontId::new(14.0, FontFamily::Proportional)
}
#[must_use]
pub fn font_body() -> FontId {
    FontId::new(13.0, FontFamily::Proportional)
}
#[must_use]
pub fn font_meta() -> FontId {
    FontId::new(11.5, FontFamily::Monospace)
}
#[must_use]
pub fn font_label() -> FontId {
    FontId::new(10.5, FontFamily::Monospace)
}

// ---------------------------------------------------------------------------
// Surfaces
// ---------------------------------------------------------------------------

/// Standard card: one hairline border, never a coloured outline.
pub fn card() -> Frame {
    Frame::NONE
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .inner_margin(Margin::same(16))
}

/// Larger workspace surface with more internal air.
pub fn panel() -> Frame {
    card().inner_margin(Margin::symmetric(18, 16))
}

/// Full-width hairline rule; the primary separator in the minimal views.
pub fn hairline(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0_f32, BORDER_SUBTLE),
    );
}

/// Two-pixel left accent used to mark selection instead of a neon outline.
pub fn accent_strip(ui: &Ui, rect: Rect, color: Color32) {
    let strip = Rect::from_min_size(rect.left_top(), egui::vec2(2.0, rect.height()));
    ui.painter()
        .rect_filled(strip, CornerRadius::same(1), color);
}

// ---------------------------------------------------------------------------
// Semantic colour
// ---------------------------------------------------------------------------

#[must_use]
pub fn status_color(status: SessionStatus) -> Color32 {
    match status {
        SessionStatus::Active => GREEN,
        SessionStatus::Idle => BLUE,
        SessionStatus::Paused => ORANGE,
        SessionStatus::NeedsReview => PURPLE,
        SessionStatus::Completed => TEXT_SECONDARY,
        SessionStatus::Failed => RED,
    }
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/// Installs fonts, text styles, spacing, and visuals. Call once at startup:
/// re-uploading fonts every frame is wasteful.
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);

    ctx.style_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, font_title()),
            (TextStyle::Body, font_body()),
            (
                TextStyle::Button,
                FontId::new(12.5, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(12.0, FontFamily::Monospace),
            ),
        ]
        .into();

        let spacing = &mut style.spacing;
        spacing.item_spacing = egui::vec2(SPACE_SM, SPACE_SM);
        spacing.button_padding = egui::vec2(10.0, 5.0);
        spacing.menu_margin = Margin::same(8);
        spacing.interact_size.y = 26.0;
        spacing.scroll.bar_width = 8.0;
        spacing.scroll.bar_inner_margin = 2.0;
        spacing.window_margin = Margin::same(12);

        style.visuals = visuals();
    });
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/InterVariable.ttf")).into(),
    );
    fonts.font_data.insert(
        "JetBrainsMono".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMonoVariable.ttf"))
            .into(),
    );
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["Inter".into()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["JetBrainsMono".into()]);
    ctx.set_fonts(fonts);
}

fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.dark_mode = true;
    visuals.panel_fill = APP_BG;
    visuals.window_fill = SURFACE;
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.faint_bg_color = SURFACE_ELEVATED;
    visuals.extreme_bg_color = CODE_BG;
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.button_frame = true;
    visuals.window_corner_radius = CornerRadius::same(RADIUS_CARD);
    visuals.menu_corner_radius = CornerRadius::same(RADIUS_CONTROL);
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.selection = egui::style::Selection {
        bg_fill: SURFACE_SELECTED,
        stroke: Stroke::new(1.0_f32, BLUE),
    };

    let radius = CornerRadius::same(RADIUS_CONTROL);
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = SURFACE;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_SECONDARY);
    widgets.noninteractive.corner_radius = radius;

    widgets.inactive.weak_bg_fill = SURFACE_ELEVATED;
    widgets.inactive.bg_fill = SURFACE_ELEVATED;
    widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_SECONDARY);
    widgets.inactive.corner_radius = radius;

    widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    widgets.hovered.bg_fill = SURFACE_HOVER;
    widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BORDER);
    widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    widgets.hovered.corner_radius = radius;

    // Pressed state stays neutral: no filled blue buttons anywhere in the shell.
    widgets.active.weak_bg_fill = SURFACE_SELECTED;
    widgets.active.bg_fill = SURFACE_SELECTED;
    widgets.active.bg_stroke = Stroke::new(1.0_f32, BLUE);
    widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    widgets.active.corner_radius = radius;

    widgets.open.weak_bg_fill = SURFACE_ELEVATED;
    widgets.open.bg_stroke = Stroke::new(1.0_f32, BORDER);
    widgets.open.corner_radius = radius;
    visuals
}
