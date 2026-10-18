//! The app's look: the desk the pages sit on, quiet panels, and the suite's rose accent
//! (provisional brand, `website/brand/storyteller`). Pages are always white paper.

use egui::{Color32, CornerRadius, Stroke, Theme};

/// The accent in light mode (#AE3D56) and in dark mode (#EE8DA2).
pub const ACCENT: Color32 = Color32::from_rgb(0xAE, 0x3D, 0x56);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0xEE, 0x8D, 0xA2);
/// A find match on the page, which is always white paper: the accent as a light wash.
pub const FIND_MARK: Color32 = Color32::from_rgb(0xEB, 0xBC, 0xC8);

/// Height of the bar along the top of the window (the title bar on macOS).
pub const TOP_BAR_HEIGHT: f32 = 38.0;

/// The background the pages sit on.
pub fn desk(dark: bool) -> Color32 {
    if dark { Color32::from_rgb(0x26, 0x26, 0x29) } else { Color32::from_rgb(0xE4, 0xE4, 0xE7) }
}

/// Faint ink for page numbers, hints and non-printing elements.
pub fn faint_ink() -> Color32 {
    Color32::from_gray(150)
}

/// Sets both themes' panels, corners and accent.
pub fn apply(ctx: &egui::Context) {
    for theme in [Theme::Light, Theme::Dark] {
        ctx.style_mut_of(theme, |style| {
            let dark = theme == Theme::Dark;
            let accent = if dark { ACCENT_DARK } else { ACCENT };
            let visuals = &mut style.visuals;
            visuals.panel_fill = if dark { Color32::from_rgb(0x1E, 0x1E, 0x20) } else { Color32::from_rgb(0xF6, 0xF6, 0xF7) };
            visuals.window_fill = if dark { Color32::from_rgb(0x2A, 0x2A, 0x2D) } else { Color32::WHITE };
            visuals.extreme_bg_color = desk(dark);
            visuals.selection.bg_fill = accent.gamma_multiply(if dark { 0.45 } else { 0.25 });
            visuals.selection.stroke = Stroke::new(1.0, accent);
            visuals.hyperlink_color = accent;
            visuals.window_corner_radius = CornerRadius::same(10);
            visuals.menu_corner_radius = CornerRadius::same(8);
            for widget in [
                &mut visuals.widgets.noninteractive,
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
                &mut visuals.widgets.open,
            ] {
                widget.corner_radius = CornerRadius::same(6);
            }
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, if dark { Color32::from_gray(52) } else { Color32::from_gray(222) });
            style.spacing.button_padding = egui::vec2(8.0, 3.0);
            style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        });
    }
}
