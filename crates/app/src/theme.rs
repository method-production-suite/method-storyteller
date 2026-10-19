//! The app's look, as named tokens: the Method family's warm paper and ink neutrals (the same
//! values as Method Budget and Method Schedule), Storyteller's own rose accent, one type scale
//! and one spacing scale. docs/BRAND.md lists every token with its measured contrast; the test
//! at the bottom keeps those numbers honest. Pages are always white paper with black type.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme};

/// One theme's colors. `LIGHT` and `DARK` are the two sets; `palette` picks one.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// The surface the pages sit on.
    pub desk: Color32,
    /// The top bar, the navigator, the find bar and the status bar.
    pub panel: Color32,
    /// Dialogs, Help windows, menus and popups.
    pub window: Color32,
    /// Text fields and other inset areas.
    pub field: Color32,
    /// Buttons at rest.
    pub control: Color32,
    /// Buttons under the pointer.
    pub control_hover: Color32,
    /// Text.
    pub ink: Color32,
    /// Secondary text.
    #[allow(dead_code, reason = "a family token; the empty and error states use it")]
    pub ink_soft: Color32,
    /// Captions, hints, counts.
    pub muted: Color32,
    /// Dividers (decorative).
    pub rule: Color32,
    /// Borders that carry meaning: a focused field, a control's edge.
    pub rule_strong: Color32,
    /// Selection, focus, links, a chosen toggle, the mark.
    pub accent: Color32,
    /// The accent's text on `accent_soft`.
    pub accent_strong: Color32,
    /// Text on an accent fill.
    #[allow(dead_code, reason = "a family token; nothing in the app fills with the accent yet")]
    pub accent_ink: Color32,
    /// Selection and highlight fill.
    pub accent_soft: Color32,
    /// Amber: caution, notes that ask for care. Never an accent.
    pub note: Color32,
    /// Errors: a file that will not open, a failed save.
    pub error: Color32,
}

pub const LIGHT: Palette = Palette {
    desk: rgb(0xE8E5DD),
    panel: rgb(0xF7F6F2),
    window: rgb(0xFFFFFF),
    field: rgb(0xFFFFFF),
    control: rgb(0xEFEDE7),
    control_hover: rgb(0xE6E3DB),
    ink: rgb(0x1C2228),
    ink_soft: rgb(0x3A424C),
    muted: rgb(0x59626D),
    rule: rgb(0xDEDBD3),
    rule_strong: rgb(0x8F897C),
    accent: rgb(0xAE3D56),
    accent_strong: rgb(0x8C2F44),
    accent_ink: rgb(0xFFFFFF),
    accent_soft: rgb(0xF6E3E8),
    note: rgb(0x8A5A00),
    error: rgb(0xB3261E),
};

pub const DARK: Palette = Palette {
    desk: rgb(0x111417),
    panel: rgb(0x1B1F24),
    window: rgb(0x22272E),
    field: rgb(0x111417),
    control: rgb(0x2B3138),
    control_hover: rgb(0x353C45),
    ink: rgb(0xE4E8EC),
    ink_soft: rgb(0xC3CAD2),
    muted: rgb(0x9AA3AD),
    rule: rgb(0x2B3138),
    rule_strong: rgb(0x67717D),
    accent: rgb(0xEE8DA2),
    accent_strong: rgb(0xF6B8C6),
    accent_ink: rgb(0x2A0E16),
    accent_soft: rgb(0x47222D),
    note: rgb(0xE3B341),
    error: rgb(0xFF9C9C),
};

/// The palette for a theme.
pub fn palette(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

/// The page, in both themes: white paper, as a printed script is.
pub const PAGE: Color32 = Color32::WHITE;
/// Type on the page.
pub const PAGE_INK: Color32 = rgb(0x1A1A1A);
/// Page and scene numbers in the margins, and an empty line's element name.
pub const PAGE_MUTED: Color32 = rgb(0x707070);
/// The dashed rule where one page ends in the draft view (decorative).
pub const PAGE_RULE: Color32 = rgb(0xC4C4C4);
/// A find match on the page: the accent as a light wash.
pub const FIND_MARK: Color32 = rgb(0xEBBCC8);

// The type scale, in points. The interface uses the system's own faces; the script is
// Courier Prime at 12 pt, set by the editor.
/// Section labels, counts and fine print.
pub const TYPE_SMALL: f32 = 11.0;
/// Labels, buttons and lists.
pub const TYPE_BODY: f32 = 13.0;
/// The script's name in the top bar.
pub const TYPE_TITLE: f32 = 13.5;
/// Headings in dialogs and Help windows.
pub const TYPE_HEADING: f32 = 17.0;
/// Code and the changelog's plain text.
pub const TYPE_MONO: f32 = 12.0;

// The spacing scale, in points.
pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;

/// Height of the bar along the top of the window (the title bar on macOS).
pub const TOP_BAR_HEIGHT: f32 = 38.0;
/// Height of one scene in the navigator.
pub const NAV_ROW_HEIGHT: f32 = 24.0;
/// Corners: controls, menus and windows.
pub const RADIUS_CONTROL: u8 = 6;
pub const RADIUS_MENU: u8 = 8;
pub const RADIUS_WINDOW: u8 = 10;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Sets both themes' type, spacing, surfaces and accent from the tokens.
pub fn apply(ctx: &egui::Context) {
    for theme in [Theme::Light, Theme::Dark] {
        ctx.style_mut_of(theme, |style| {
            let p = palette(theme == Theme::Dark);
            style.text_styles = [
                (TextStyle::Small, FontId::new(TYPE_SMALL, FontFamily::Proportional)),
                (TextStyle::Body, FontId::new(TYPE_BODY, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(TYPE_BODY, FontFamily::Proportional)),
                (TextStyle::Heading, FontId::new(TYPE_HEADING, FontFamily::Proportional)),
                (TextStyle::Monospace, FontId::new(TYPE_MONO, FontFamily::Monospace)),
            ]
            .into();
            style.spacing.button_padding = egui::vec2(SPACE_2, 3.0);
            style.spacing.item_spacing = egui::vec2(SPACE_2, SPACE_1);
            style.spacing.window_margin = egui::Margin::same(SPACE_4 as i8);
            style.spacing.menu_margin = egui::Margin::same(SPACE_1 as i8 + 2);

            let visuals = &mut style.visuals;
            visuals.panel_fill = p.panel;
            visuals.window_fill = p.window;
            visuals.window_stroke = Stroke::new(1.0, p.rule);
            visuals.extreme_bg_color = p.desk;
            visuals.text_edit_bg_color = Some(p.field);
            visuals.faint_bg_color = p.control;
            visuals.code_bg_color = p.control;
            visuals.weak_text_color = Some(p.muted);
            visuals.warn_fg_color = p.note;
            visuals.error_fg_color = p.error;
            visuals.hyperlink_color = p.accent;
            visuals.selection.bg_fill = p.accent_soft;
            visuals.selection.stroke = Stroke::new(1.0, p.accent_strong);
            visuals.window_corner_radius = CornerRadius::same(RADIUS_WINDOW);
            visuals.menu_corner_radius = CornerRadius::same(RADIUS_MENU);

            let widgets = &mut visuals.widgets;
            widgets.noninteractive.bg_fill = p.panel;
            widgets.noninteractive.weak_bg_fill = p.panel;
            widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.rule);
            widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.ink);
            for (widget, fill) in
                [(&mut widgets.inactive, p.control), (&mut widgets.hovered, p.control_hover), (&mut widgets.active, p.control_hover)]
            {
                widget.bg_fill = fill;
                widget.weak_bg_fill = fill;
                widget.fg_stroke = Stroke::new(1.0, p.ink);
            }
            widgets.inactive.bg_stroke = Stroke::NONE;
            widgets.hovered.bg_stroke = Stroke::new(1.0, p.rule_strong);
            widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
            widgets.open.bg_fill = p.control_hover;
            widgets.open.weak_bg_fill = p.control_hover;
            widgets.open.bg_stroke = Stroke::new(1.0, p.rule_strong);
            widgets.open.fg_stroke = Stroke::new(1.0, p.ink);
            for widget in [&mut widgets.noninteractive, &mut widgets.inactive, &mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
                widget.corner_radius = CornerRadius::same(RADIUS_CONTROL);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2.x contrast ratio.
    pub(crate) fn contrast(a: Color32, b: Color32) -> f32 {
        fn channel(c: u8) -> f32 {
            let c = f32::from(c) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        fn luminance(c: Color32) -> f32 {
            0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
        }
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Every pair BRAND.md lists, with what it needs: 4.5 for text, 3 for meaningful borders.
    fn pairs(p: &Palette) -> Vec<(&'static str, Color32, Color32, f32)> {
        vec![
            ("ink on panel", p.ink, p.panel, 4.5),
            ("ink on window", p.ink, p.window, 4.5),
            ("ink on control", p.ink, p.control, 4.5),
            ("ink on control-hover", p.ink, p.control_hover, 4.5),
            ("ink on accent-soft", p.ink, p.accent_soft, 4.5),
            ("ink-soft on panel", p.ink_soft, p.panel, 4.5),
            ("muted on panel", p.muted, p.panel, 4.5),
            ("muted on window", p.muted, p.window, 4.5),
            ("muted on desk", p.muted, p.desk, 4.5),
            ("muted on accent-soft", p.muted, p.accent_soft, 4.5),
            ("accent on panel", p.accent, p.panel, 4.5),
            ("accent on window", p.accent, p.window, 4.5),
            ("accent on desk", p.accent, p.desk, 4.5),
            ("accent-strong on accent-soft", p.accent_strong, p.accent_soft, 4.5),
            ("accent-ink on accent", p.accent_ink, p.accent, 4.5),
            ("note on window", p.note, p.window, 4.5),
            ("error on window", p.error, p.window, 4.5),
            ("error on panel", p.error, p.panel, 4.5),
            ("rule-strong on panel", p.rule_strong, p.panel, 3.0),
        ]
    }

    #[test]
    fn every_pair_meets_wcag_aa() {
        let mut failures = Vec::new();
        for (name, palette) in [("light", &LIGHT), ("dark", &DARK)] {
            for (pair, fg, bg, needs) in pairs(palette) {
                let ratio = contrast(fg, bg);
                if ratio < needs {
                    failures.push(format!("{pair} ({name}): {ratio:.2}, needs {needs}"));
                }
            }
        }
        for (pair, fg, bg) in
            [("page ink on page", PAGE_INK, PAGE), ("page muted on page", PAGE_MUTED, PAGE), ("page ink on find mark", PAGE_INK, FIND_MARK)]
        {
            let ratio = contrast(fg, bg);
            if ratio < 4.5 {
                failures.push(format!("{pair}: {ratio:.2}, needs 4.5"));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// Prints the table BRAND.md carries: `cargo test -p method-storyteller contrast_table -- --nocapture`.
    #[test]
    fn contrast_table() {
        for ((pair, fg, bg, needs), (_, dfg, dbg, _)) in pairs(&LIGHT).into_iter().zip(pairs(&DARK)) {
            println!("| {pair} | {:.2} | {:.2} | {needs} |", contrast(fg, bg), contrast(dfg, dbg));
        }
        for (pair, fg, bg) in
            [("page ink on page", PAGE_INK, PAGE), ("page muted on page", PAGE_MUTED, PAGE), ("page ink on find mark", PAGE_INK, FIND_MARK)]
        {
            println!("| {pair} | {:.2} | {:.2} | 4.5 |", contrast(fg, bg), contrast(fg, bg));
        }
    }
}
