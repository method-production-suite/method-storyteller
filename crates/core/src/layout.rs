//! Page geometry and element margins.
//!
//! Units are character cells: a column is 1/10 inch (Courier 12 pt is 10 characters per inch)
//! and a row is 1/6 inch (12 pt). US Letter is 85 × 66 cells. The defaults follow the element
//! settings Final Draft writes into its own files (docs/spec.md §5); an imported `.fdx`
//! brings its own (see [`Layout::for_script`]).

use serde::{Deserialize, Serialize};

use crate::model::ElementKind;

/// Points per column (1/10 inch).
pub const POINTS_PER_COLUMN: f32 = 7.2;
/// Points per row (1/6 inch).
pub const POINTS_PER_ROW: f32 = 12.0;
/// How far Final Draft advances per character when wrapping, measured from its PDFs: a 6.00"
/// action line holds 61 characters and a 3.50" dialogue line 35, so the advance lies between
/// 7.00 and 7.08 points, a little under Courier's nominal 7.2.
pub const WRAP_ADVANCE_POINTS: f32 = 7.04;

/// Characters that fit on a line `inches` wide.
pub fn chars_per_line(inches: f32) -> u16 {
    if !inches.is_finite() || inches <= 0.0 {
        return 1;
    }
    ((inches * 72.0 / WRAP_ADVANCE_POINTS) + 1e-4).floor().clamp(1.0, 500.0) as u16
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Right,
    Center,
}

/// Where and how an element is printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementStyle {
    /// First column, from the left edge of the page.
    pub left: u16,
    /// Wrap width in characters.
    pub width: u16,
    pub align: Align,
    /// Blank lines before the element (dropped at the top of a page).
    pub space_before: u16,
}

impl ElementStyle {
    const fn new(left: u16, width: u16, align: Align, space_before: u16) -> Self {
        ElementStyle { left, width, align, space_before }
    }
}

/// Margins for one side of a dual-dialogue pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DualColumn {
    pub character: ElementStyle,
    pub parenthetical: ElementStyle,
    pub dialogue: ElementStyle,
}

/// The complete page layout. [`Layout::default`] is the US Letter industry standard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    pub page_columns: u16,
    pub page_rows: u16,
    /// First body row (1 inch from the top).
    pub body_top: u16,
    pub lines_per_page: u16,
    /// Row of the page number (0.5 inch from the top).
    pub page_number_row: u16,
    /// Column after the last character of the page number (7.5 inches).
    pub page_number_right: u16,
    /// Column after the last character of the left scene number.
    pub scene_number_left_end: u16,
    /// First column of the right scene number.
    pub scene_number_right: u16,
    pub show_scene_numbers: bool,
    pub more: String,
    pub contd: String,
    pub scene_heading: ElementStyle,
    pub action: ElementStyle,
    pub character: ElementStyle,
    pub parenthetical: ElementStyle,
    pub dialogue: ElementStyle,
    pub transition: ElementStyle,
    pub shot: ElementStyle,
    pub centered: ElementStyle,
    pub lyrics: ElementStyle,
    pub dual_left: DualColumn,
    pub dual_right: DualColumn,
    /// Styles of other paragraph types from an imported file, such as `New Act` or `General`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<(String, ElementStyle)>,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            page_columns: 85,
            page_rows: 66,
            body_top: 6,
            lines_per_page: 54,
            page_number_row: 3,
            page_number_right: 75,
            scene_number_left_end: 12,
            scene_number_right: 74,
            show_scene_numbers: true,
            more: "(MORE)".into(),
            contd: "(CONT'D)".into(),
            // Widths are characters per line (see `chars_per_line`), not columns.
            scene_heading: ElementStyle::new(15, 61, Align::Left, 2),
            action: ElementStyle::new(15, 61, Align::Left, 1),
            character: ElementStyle::new(35, 38, Align::Left, 1),
            // The first line hangs one column left of the rest (Final Draft's -0.10" first indent).
            parenthetical: ElementStyle::new(29, 25, Align::Left, 0),
            dialogue: ElementStyle::new(25, 35, Align::Left, 0),
            transition: ElementStyle::new(55, 16, Align::Right, 1),
            shot: ElementStyle::new(15, 61, Align::Left, 2),
            centered: ElementStyle::new(15, 61, Align::Center, 1),
            lyrics: ElementStyle::new(25, 35, Align::Left, 1),
            dual_left: DualColumn {
                character: ElementStyle::new(22, 20, Align::Left, 1),
                parenthetical: ElementStyle::new(18, 22, Align::Left, 0),
                dialogue: ElementStyle::new(15, 28, Align::Left, 0),
            },
            dual_right: DualColumn {
                character: ElementStyle::new(53, 20, Align::Left, 1),
                parenthetical: ElementStyle::new(49, 22, Align::Left, 0),
                dialogue: ElementStyle::new(46, 28, Align::Left, 0),
            },
            custom: Vec::new(),
        }
    }
}

impl Layout {
    /// The layout a script paginates with: the default, with an imported `.fdx` file's own
    /// page layout and element settings applied.
    pub fn for_script(script: &crate::model::Script) -> Layout {
        let mut layout = Layout::default();
        if let Some(document) = &script.fdx {
            crate::fdx::apply_layout(document, &mut layout);
        }
        if let Some(document) = &script.osf {
            crate::osf::apply_layout(document, &mut layout);
        }
        layout
    }

    /// Style of a paragraph type from an imported file, by its Final Draft name.
    pub fn custom_style(&self, type_name: &str) -> Option<ElementStyle> {
        self.custom.iter().find(|(name, _)| name == type_name).map(|(_, style)| *style)
    }

    /// Style of a printed element kind (non-printed kinds get the action style).
    pub fn style(&self, kind: ElementKind) -> ElementStyle {
        match kind {
            ElementKind::SceneHeading => self.scene_heading,
            ElementKind::Character => self.character,
            ElementKind::Parenthetical => self.parenthetical,
            ElementKind::Dialogue => self.dialogue,
            ElementKind::Transition => self.transition,
            ElementKind::Shot => self.shot,
            ElementKind::Centered => self.centered,
            ElementKind::Lyrics => self.lyrics,
            ElementKind::Action | ElementKind::Section | ElementKind::Synopsis | ElementKind::PageBreak => self.action,
        }
    }

    /// Page width and height in points.
    pub fn page_size_points(&self) -> (f32, f32) {
        (f32::from(self.page_columns) * POINTS_PER_COLUMN, f32::from(self.page_rows) * POINTS_PER_ROW)
    }
}
