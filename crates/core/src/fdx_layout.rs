//! Layout: element settings read as styles.

use super::*;

/// An indent in inches.
pub(super) fn inches(value: &str) -> Option<f32> {
    let value: f32 = value.trim().parse().ok()?;
    (value.is_finite() && (-10.0..=20.0).contains(&value)).then_some(value)
}

/// Inches to character columns (10 per inch).
pub(super) fn columns(inches: f32) -> u16 {
    (inches.max(0.0) * 10.0).round() as u16
}

/// Points to lines (12 points each).
pub(super) fn lines(points: &str) -> Option<u16> {
    let value: f32 = points.trim().parse().ok()?;
    (value.is_finite() && (0.0..=720.0).contains(&value)).then(|| (value / 12.0).round() as u16)
}

/// One element's settings as Final Draft wrote them.
#[derive(Default)]
pub(super) struct ElementSpec {
    pub(super) left: Option<f32>,
    pub(super) first_indent: Option<f32>,
    pub(super) right: Option<f32>,
    pub(super) space_before: Option<u16>,
    pub(super) align: Option<Align>,
}

impl ElementSpec {
    /// Applies the spec over `style`. Final Draft's first indent is relative to the left
    /// indent; a negative one (parentheticals) hangs the first line to the left.
    fn apply(&self, style: &mut ElementStyle) {
        if let (Some(left), Some(right)) = (self.left, self.right) {
            let first = self.first_indent.unwrap_or(0.0).min(0.0);
            style.left = columns(left + first);
            style.width = chars_per_line(right - left);
        }
        if let Some(space) = self.space_before {
            style.space_before = space;
        }
        if let Some(align) = self.align {
            style.align = align;
        }
    }
}

pub(super) fn align_from(value: &str) -> Option<Align> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" => Some(Align::Left),
        "right" => Some(Align::Right),
        "center" => Some(Align::Center),
        _ => None,
    }
}

/// An imported paragraph's own formatting (Final Draft writes `LeftIndent`, `SpaceBefore` and so
/// on on paragraphs that override their element settings) applied over `base`.
pub fn paragraph_style(fdx: &FdxParagraph, base: ElementStyle) -> ElementStyle {
    let attributes = &fdx.attributes;
    if !attributes.iter().any(|(key, _)| matches!(key.as_str(), "LeftIndent" | "RightIndent" | "SpaceBefore" | "Alignment")) {
        return base;
    }
    let spec = ElementSpec {
        left: get(attributes, "LeftIndent").and_then(inches),
        right: get(attributes, "RightIndent").and_then(inches),
        first_indent: get(attributes, "FirstIndent").and_then(inches),
        space_before: get(attributes, "SpaceBefore").and_then(lines),
        align: get(attributes, "Alignment").and_then(align_from),
    };
    let mut style = base;
    spec.apply(&mut style);
    style
}

/// Reads one top-level section's start tags: `(tag name, attributes)` in order.
pub(super) fn tags(xml: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out = Vec::new();
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(start) | Event::Empty(start) => {
                out.push((start.local_name().as_ref().to_string(), all_attrs(&start)));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

pub(super) fn get<'a>(attributes: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attributes.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}
