//! Element settings and page layout, as both `.fdx` and Open Screenplay Format hold them.
//!
//! [`Format`] is what the two formats share: page size and margins, the (MORE) and (CONT'D)
//! texts, and for each element type its font, styles, alignment, indents, space before, line
//! spacing, a page break before, and the elements Enter and Tab lead to. Lengths are kept in
//! thousandths of an inch, which holds both `.fdx` inches and OSF tenths of a millimetre exactly.
//!
//! Everything else each format says about an element (`.fdx` Behavior shortcuts, outline levels,
//! OSF keep-with-next …) is kept as the XML it came in, and rides along to the other format in an
//! attribute (`fdx_xml` on an OSF style, `StorytellerOsf` on `.fdx` settings), so a round trip
//! through the other format comes back the same.

use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

use crate::fdx::{all_attrs, escape};
use crate::layout::{Align, ElementStyle, Layout, chars_per_line};

/// A length in thousandths of an inch.
pub type Mils = i32;

/// Page layout and element settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Format {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<PageFormat>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<ElementFormat>,
}

impl Format {
    pub fn is_empty(&self) -> bool {
        self.page.is_none() && self.elements.is_empty()
    }

    /// The settings of an element type, by name.
    pub fn element(&self, name: &str) -> Option<&ElementFormat> {
        self.elements.iter().find(|e| e.name.eq_ignore_ascii_case(name))
    }
}

/// Page size, margins and the mores and continueds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageFormat {
    pub width: Option<Mils>,
    pub height: Option<Mils>,
    pub top: Option<Mils>,
    pub bottom: Option<Mils>,
    /// Left and right page margins (OSF; `.fdx` measures indents from the paper's edge instead).
    pub left: Option<Mils>,
    pub right: Option<Mils>,
    /// The text at the foot of a speech that breaks, such as `(MORE)`.
    pub more: Option<String>,
    /// The text after a cue continued from the page before, such as `(CONT'D)`.
    pub contd: Option<String>,
    /// Add `contd` when the same character speaks again in a scene.
    pub auto_contd: Option<bool>,
    /// `.fdx` `PageLayout` and `MoresAndContinueds` as read, the base for writing them.
    #[serde(skip)]
    pub fdx_page_layout: String,
    #[serde(skip)]
    pub fdx_mores: String,
    /// OSF `settings` as read, the base for writing them.
    #[serde(skip)]
    pub osf_settings: String,
}

/// How one element type prints and flows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementFormat {
    /// `.fdx` type name or OSF style name, such as `Scene Heading` or `New Act`.
    pub name: String,
    /// The built-in type it paginates like (`.fdx` `PaginateAs`, OSF `basestylename`).
    pub based_on: Option<String>,
    pub font: Option<String>,
    /// Points.
    pub size: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub all_caps: bool,
    /// Never printed (`.fdx` `HiddenText`).
    pub hidden: bool,
    pub align: Option<Align>,
    /// First column of the text, from the paper's left edge.
    pub left: Option<Mils>,
    /// End of the text, from the paper's left edge.
    pub right: Option<Mils>,
    /// First line's indent from `left` (negative hangs it to the left).
    pub first: Option<Mils>,
    /// Points before the paragraph (12 a line).
    pub space_before: Option<u32>,
    /// Line spacing in hundredths of a line (100 single).
    pub spacing: Option<u32>,
    /// Starts a new page.
    pub new_page: bool,
    /// The element Enter and Tab lead to.
    pub enter: Option<String>,
    pub tab: Option<String>,
    /// `.fdx` `ElementSettings` as read, the base for writing it.
    #[serde(skip)]
    pub fdx_xml: String,
    /// OSF `<style>` as read, the base for writing it.
    #[serde(skip)]
    pub osf_xml: String,
}

// ---------------------------------------------------------------------------------------------
// A small tree of elements and attributes, for patching settings sections.

#[derive(Clone, Debug, Default)]
struct Node {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<Node>,
}

impl Node {
    fn new(name: &str) -> Node {
        Node { name: name.to_string(), ..Node::default() }
    }

    fn parse(xml: &str) -> Option<Node> {
        let mut reader = quick_xml::Reader::from_str(xml);
        let mut stack: Vec<Node> = Vec::new();
        loop {
            match reader.read_event().ok()? {
                Event::Start(start) => {
                    if stack.len() >= 32 {
                        return None;
                    }
                    stack.push(Node {
                        name: String::from_utf8_lossy(start.name().as_ref()).into_owned(),
                        attributes: all_attrs(&start),
                        children: Vec::new(),
                    })
                }
                Event::Empty(start) => {
                    let node = Node {
                        name: String::from_utf8_lossy(start.name().as_ref()).into_owned(),
                        attributes: all_attrs(&start),
                        children: Vec::new(),
                    };
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(node),
                        None => return Some(node),
                    }
                }
                Event::End(_) => {
                    let node = stack.pop()?;
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(node),
                        None => return Some(node),
                    }
                }
                Event::Eof => return None,
                _ => {}
            }
        }
    }

    fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (key, value) in &self.attributes {
            write_attr(key, value, out);
        }
        if self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for child in &self.children {
            child.write(out);
        }
        out.push_str(&format!("</{}>", self.name));
    }

    fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.attributes.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// The child `name`, added in its place in `order` if missing.
    fn child_mut(&mut self, name: &str, order: &[&str]) -> Option<&mut Node> {
        if !self.children.iter().any(|c| c.name == name) {
            let rank = |n: &str| order.iter().position(|o| *o == n).unwrap_or(order.len());
            let at = self.children.iter().position(|c| rank(&c.name) > rank(name)).unwrap_or(self.children.len());
            self.children.insert(at, Node::new(name));
        }
        self.children.iter_mut().find(|c| c.name == name)
    }

    /// Sets `key`, unless what it holds already `means` the value; `None` removes it.
    fn set(&mut self, key: &str, value: Option<String>, means: impl Fn(&str) -> bool) {
        let at = self.attributes.iter().position(|(k, _)| k == key);
        match (at, value) {
            (Some(at), _) if self.attributes.get(at).is_some_and(|(_, v)| means(v)) => {}
            (Some(at), Some(value)) => {
                if let Some((_, slot)) = self.attributes.get_mut(at) {
                    *slot = value;
                }
            }
            (Some(at), None) => {
                self.attributes.remove(at);
            }
            (None, Some(value)) => self.attributes.push((key.to_string(), value)),
            (None, None) => {}
        }
    }
}

fn write_attr(key: &str, value: &str, out: &mut String) {
    out.push(' ');
    out.push_str(key);
    out.push_str("=\"");
    for c in value.chars() {
        match c {
            '\n' => out.push_str("&#10;"),
            '\t' => out.push_str("&#9;"),
            '\r' => out.push_str("&#13;"),
            c => escape(c.encode_utf8(&mut [0; 4]), out),
        }
    }
    out.push('"');
}

// ---------------------------------------------------------------------------------------------
// Units

fn number(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok().filter(|v| v.is_finite() && v.abs() < 1_000_000.0)
}

fn mils_from_inches(value: &str) -> Option<Mils> {
    number(value).map(|v| (v * 1000.0).round() as Mils)
}

fn inches(mils: Mils) -> String {
    if mils % 10 == 0 { format!("{:.2}", f64::from(mils) / 1000.0) } else { format!("{:.3}", f64::from(mils) / 1000.0) }
}

fn mils_from_points(value: &str) -> Option<Mils> {
    number(value).map(|v| (v * 1000.0 / 72.0).round() as Mils)
}

fn points(mils: Mils) -> String {
    let value = f64::from(mils) * 72.0 / 1000.0;
    if (value - value.round()).abs() < 1e-9 { format!("{}", value.round() as i64) } else { format!("{value:.2}") }
}

fn mils_from_tenth_mm(value: &str) -> Option<Mils> {
    number(value).map(|v| (v * 1000.0 / 254.0).round() as Mils)
}

fn tenth_mm(mils: Mils) -> String {
    format!("{}", (f64::from(mils) * 254.0 / 1000.0).round() as i64)
}

/// A count in the format's own decimal spelling (`2`, `1.5`).
fn decimal(hundredths: u32) -> String {
    let value = f64::from(hundredths) / 100.0;
    if hundredths.is_multiple_of(100) { format!("{}", hundredths / 100) } else { format!("{value}") }
}

fn hundredths(value: &str) -> Option<u32> {
    number(value).filter(|v| *v >= 0.0).map(|v| (v * 100.0).round() as u32)
}

fn truthy(value: Option<&str>) -> bool {
    value.is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "yes" | "true"))
}

fn align_from(value: &str) -> Option<Align> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" => Some(Align::Left),
        "right" => Some(Align::Right),
        "center" => Some(Align::Center),
        _ => None,
    }
}

fn align_name(align: Align, capital: bool) -> &'static str {
    match (align, capital) {
        (Align::Left, true) => "Left",
        (Align::Right, true) => "Right",
        (Align::Center, true) => "Center",
        (Align::Left, false) => "left",
        (Align::Right, false) => "right",
        (Align::Center, false) => "center",
    }
}

/// `(CONT'D)` for `CONT'D` or `(CONT'D)`.
fn bracketed(text: &str) -> String {
    format!("({})", text.trim().trim_start_matches('(').trim_end_matches(')'))
}

// ---------------------------------------------------------------------------------------------
// Final Draft

/// The attribute carrying an OSF style or settings tag through an `.fdx`.
const OSF_ATTR: &str = "StorytellerOsf";
const ELEMENT_ORDER: [&str; 4] = ["FontSpec", "ParagraphSpec", "Behavior", "Outline"];

/// The format held by an `.fdx` file's `ElementSettings`, `PageLayout` and `MoresAndContinueds`.
pub(crate) fn from_fdx<'a>(sections: impl Iterator<Item = &'a str>) -> Format {
    let mut format = Format::default();
    for xml in sections {
        let Some(node) = Node::parse(xml) else { continue };
        match node.name.as_str() {
            "ElementSettings" => format.elements.push(element_from_fdx(&node, xml)),
            "PageLayout" => {
                let page = format.page.get_or_insert_with(PageFormat::default);
                let size = node.child("PageSize");
                page.width = size.and_then(|s| s.get("Width")).and_then(mils_from_inches);
                page.height = size.and_then(|s| s.get("Height")).and_then(mils_from_inches);
                page.top = node.get("TopMargin").and_then(mils_from_points);
                page.bottom = node.get("BottomMargin").and_then(mils_from_points);
                page.fdx_page_layout = xml.to_string();
                if let Some(osf) = node.get(OSF_ATTR) {
                    page.osf_settings = osf.to_string();
                    // Margins an OSF document had of its own; ones filled in when this file was
                    // written to OSF (which then carried this layout) do not count.
                    let carried = Node::parse(osf).unwrap_or_default();
                    if carried.get("fdx_page_layout").is_none() {
                        page.left = carried.get("margin_left").and_then(mils_from_tenth_mm);
                        page.right = carried.get("margin_right").and_then(mils_from_tenth_mm);
                    }
                }
            }
            "MoresAndContinueds" => {
                let page = format.page.get_or_insert_with(PageFormat::default);
                let breaks = node.child("DialogueBreaks");
                page.more = breaks.and_then(|b| b.get("DialogueBottom")).map(str::to_string);
                page.contd = breaks.and_then(|b| b.get("DialogueTop")).map(str::to_string);
                page.auto_contd = breaks.and_then(|b| b.get("AutomaticCharacterContinueds")).map(|v| truthy(Some(v)));
                page.fdx_mores = xml.to_string();
            }
            _ => {}
        }
    }
    format
}

fn element_from_fdx(node: &Node, xml: &str) -> ElementFormat {
    let font = node.child("FontSpec");
    let paragraph = node.child("ParagraphSpec");
    let behavior = node.child("Behavior");
    let style = font.and_then(|f| f.get("Style")).unwrap_or_default();
    let has = |token: &str| style.split('+').any(|t| t.trim() == token);
    ElementFormat {
        name: node.get("Type").unwrap_or_default().to_string(),
        based_on: behavior.and_then(|b| b.get("PaginateAs")).map(str::to_string),
        font: font.and_then(|f| f.get("Font")).map(str::to_string),
        size: font.and_then(|f| f.get("Size")).and_then(|s| s.trim().parse().ok()),
        bold: has("Bold"),
        italic: has("Italic"),
        underline: has("Underline"),
        all_caps: has("AllCaps"),
        hidden: has("HiddenText"),
        align: paragraph.and_then(|p| p.get("Alignment")).and_then(align_from),
        left: paragraph.and_then(|p| p.get("LeftIndent")).and_then(mils_from_inches),
        right: paragraph.and_then(|p| p.get("RightIndent")).and_then(mils_from_inches),
        first: paragraph.and_then(|p| p.get("FirstIndent")).and_then(mils_from_inches),
        space_before: paragraph.and_then(|p| p.get("SpaceBefore")).and_then(number).map(|v| v.max(0.0).round() as u32),
        spacing: paragraph.and_then(|p| p.get("Spacing")).and_then(hundredths),
        new_page: truthy(paragraph.and_then(|p| p.get("StartsNewPage"))),
        enter: behavior.and_then(|b| b.get("ReturnKey")).map(str::to_string),
        tab: behavior.and_then(|b| b.get("TabKey")).map(str::to_string),
        fdx_xml: xml.to_string(),
        osf_xml: node.get(OSF_ATTR).unwrap_or_default().to_string(),
    }
}

/// The element's `.fdx` settings are more than writing them from the model would give.
fn fdx_needed(element: &ElementFormat, page: Option<&PageFormat>) -> bool {
    !element.fdx_xml.is_empty() && fdx_element(&ElementFormat { fdx_xml: String::new(), ..element.clone() }, page) != element.fdx_xml
}

/// The element's OSF style is more than writing it from the model would give.
fn osf_needed(element: &ElementFormat, page: Option<&PageFormat>) -> bool {
    let spelling = OsfSpelling { base_style: "basestylename" };
    !element.osf_xml.is_empty() && osf_style(&ElementFormat { osf_xml: String::new(), ..element.clone() }, page, &spelling) != element.osf_xml
}

/// An element's `ElementSettings`, from the one it came with brought up to date.
pub(crate) fn fdx_element(element: &ElementFormat, page: Option<&PageFormat>) -> String {
    let mut node = Node::parse(&element.fdx_xml).filter(|n| n.name == "ElementSettings").unwrap_or_else(|| Node::new("ElementSettings"));
    node.set("Type", Some(element.name.clone()), |v| v == element.name);
    if let Some(font) = node.child_mut("FontSpec", &ELEMENT_ORDER) {
        font.set("Font", element.font.clone(), |v| Some(v) == element.font.as_deref());
        font.set("Size", element.size.map(|s| s.to_string()), |v| v.trim().parse().ok() == element.size);
        let flags = [
            ("Bold", element.bold),
            ("Italic", element.italic),
            ("Underline", element.underline),
            ("AllCaps", element.all_caps),
            ("HiddenText", element.hidden),
        ];
        let current = font.get("Style").unwrap_or_default().to_string();
        let others: Vec<&str> = current.split('+').map(str::trim).filter(|t| !t.is_empty() && !flags.iter().any(|(n, _)| n == t)).collect();
        let tokens: Vec<&str> = others.into_iter().chain(flags.iter().filter(|(_, on)| *on).map(|(n, _)| *n)).collect();
        let same = |v: &str| flags.iter().all(|(n, on)| v.split('+').any(|t| t.trim() == *n) == *on);
        let value = if tokens.is_empty() && font.get("Style").is_none() { None } else { Some(tokens.join("+")) };
        font.set("Style", value, same);
    }
    if let Some(paragraph) = node.child_mut("ParagraphSpec", &ELEMENT_ORDER) {
        paragraph.set("Alignment", element.align.map(|a| align_name(a, true).to_string()), |v| align_from(v) == element.align);
        for (key, value) in [("LeftIndent", element.left), ("RightIndent", element.right), ("FirstIndent", element.first)] {
            paragraph.set(key, value.map(inches), |v| mils_from_inches(v) == value);
        }
        let space = element.space_before;
        paragraph.set("SpaceBefore", space.map(|s| s.to_string()), |v| number(v).map(|n| n.max(0.0).round() as u32) == space);
        paragraph.set("Spacing", element.spacing.map(decimal), |v| hundredths(v) == element.spacing);
        let existing = paragraph.get("StartsNewPage").is_some();
        paragraph.set("StartsNewPage", (element.new_page || existing).then(|| if element.new_page { "Yes" } else { "No" }.to_string()), |v| {
            truthy(Some(v)) == element.new_page
        });
    }
    if (element.based_on.is_some() || element.enter.is_some() || element.tab.is_some() || node.child("Behavior").is_some())
        && let Some(behavior) = node.child_mut("Behavior", &ELEMENT_ORDER)
    {
        behavior.set("PaginateAs", element.based_on.clone(), |v| Some(v) == element.based_on.as_deref());
        behavior.set("ReturnKey", element.enter.clone(), |v| Some(v) == element.enter.as_deref());
        behavior.set("TabKey", element.tab.clone(), |v| Some(v) == element.tab.as_deref());
    }
    let carried = Some(element.osf_xml.clone()).filter(|_| osf_needed(element, page));
    node.set(OSF_ATTR, carried, |v| v == element.osf_xml);
    node.to_xml()
}

/// The `PageLayout`, from the one the page came with brought up to date.
pub(crate) fn fdx_page_layout(page: &PageFormat) -> String {
    let mut node = Node::parse(&page.fdx_page_layout).filter(|n| n.name == "PageLayout").unwrap_or_else(|| Node::new("PageLayout"));
    node.set("TopMargin", page.top.map(points), |v| mils_from_points(v) == page.top);
    node.set("BottomMargin", page.bottom.map(points), |v| mils_from_points(v) == page.bottom);
    let bare = PageFormat { osf_settings: String::new(), ..page.clone() };
    let carried = Some(page.osf_settings.clone()).filter(|x| !x.is_empty() && *x != osf_settings(&bare, "<settings/>"));
    node.set(OSF_ATTR, carried, |v| v == page.osf_settings);
    if (page.width.is_some() || page.height.is_some())
        && let Some(size) = node.child_mut("PageSize", &["PageSize", "AutoCastList"])
    {
        size.set("Height", page.height.map(inches), |v| mils_from_inches(v) == page.height);
        size.set("Width", page.width.map(inches), |v| mils_from_inches(v) == page.width);
    }
    node.to_xml()
}

/// The `MoresAndContinueds`, from the one the page came with brought up to date.
pub(crate) fn fdx_mores(page: &PageFormat) -> String {
    let mut node = Node::parse(&page.fdx_mores).filter(|n| n.name == "MoresAndContinueds").unwrap_or_else(|| Node::new("MoresAndContinueds"));
    let Some(breaks) = node.child_mut("DialogueBreaks", &["FontSpec", "DialogueBreaks", "SceneBreaks"]) else { return node.to_xml() };
    breaks.set("AutomaticCharacterContinueds", page.auto_contd.map(|a| if a { "Yes" } else { "No" }.to_string()), |v| {
        Some(truthy(Some(v))) == page.auto_contd
    });
    breaks.set("DialogueBottom", page.more.clone(), |v| Some(v) == page.more.as_deref());
    breaks.set("DialogueTop", page.contd.clone(), |v| Some(v) == page.contd.as_deref());
    node.to_xml()
}

// ---------------------------------------------------------------------------------------------
// Open Screenplay Format

/// The attribute carrying `.fdx` settings through an OSF document.
const FDX_ATTR: &str = "fdx_xml";

/// An attribute compared loosely, as OSF versions spell names differently.
fn loose(name: &str) -> String {
    name.chars().filter(|c| *c != '_').flat_map(char::to_lowercase).collect()
}

fn osf_get<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let wanted = loose(name);
    attributes.iter().find(|(k, _)| loose(k) == wanted).map(|(_, v)| v.as_str())
}

/// Default OSF page margins when a document gives none: 1.5" left, 1" right, top and bottom.
const OSF_LEFT: Mils = 1500;
const OSF_RIGHT: Mils = 1000;

/// The format held by an OSF document's `settings` tag and `styles` section.
pub(crate) fn from_osf(settings: Option<&str>, styles: Option<&str>) -> Format {
    let mut format = Format::default();
    if let Some(raw) = settings
        && let Some(node) = Node::parse(raw)
    {
        let a = &node.attributes;
        let length = |name: &str| osf_get(a, name).and_then(mils_from_tenth_mm);
        format.page = Some(PageFormat {
            width: length("page_width"),
            height: length("page_height"),
            top: length("margin_top"),
            bottom: length("margin_bottom"),
            left: length("margin_left"),
            right: length("margin_right"),
            more: osf_get(a, "more_text").map(str::to_string),
            contd: osf_get(a, "cont_text").map(bracketed),
            auto_contd: osf_get(a, "dialogue_continues").map(|v| truthy(Some(v))),
            fdx_page_layout: osf_get(a, "fdx_page_layout").unwrap_or_default().to_string(),
            fdx_mores: osf_get(a, "fdx_mores").unwrap_or_default().to_string(),
            osf_settings: raw.to_string(),
        });
    }
    // A page that came from an `.fdx` takes back that file's own lengths, which are finer than
    // OSF's, wherever they round to what the OSF document says.
    if let Some(page) = format.page.as_mut()
        && !page.fdx_page_layout.is_empty()
    {
        let fdx = from_fdx([page.fdx_page_layout.as_str()].into_iter()).page.unwrap_or_default();
        // Lengths the `.fdx` did not give were only filled in for OSF.
        for (value, finer) in [(&mut page.width, fdx.width), (&mut page.height, fdx.height), (&mut page.top, fdx.top), (&mut page.bottom, fdx.bottom)]
        {
            if finer.is_none() {
                *value = None;
            }
            refine(value, finer);
        }
        // Margins the `.fdx` did not have were only filled in for OSF.
        if fdx.osf_settings.is_empty() {
            page.left = None;
            page.right = None;
        }
    }
    let page = format.page.clone().unwrap_or_default();
    let margin_left = page.left.unwrap_or(OSF_LEFT);
    let paper_right = page.width.unwrap_or(8500) - page.right.unwrap_or(OSF_RIGHT);
    for xml in styles.map(crate::fdx::child_elements).unwrap_or_default().into_iter().take(1000) {
        let Some(node) = Node::parse(xml).filter(|n| n.name.eq_ignore_ascii_case("style")) else { continue };
        let a = &node.attributes;
        let Some(name) = osf_get(a, "name") else { continue };
        let length = |name: &str| osf_get(a, name).and_then(mils_from_tenth_mm);
        format.elements.push(ElementFormat {
            name: name.to_string(),
            based_on: osf_get(a, "basestylename").or_else(|| osf_get(a, "basestyle")).map(str::to_string),
            font: osf_get(a, "font").map(str::to_string),
            size: osf_get(a, "size").and_then(number).map(|s| s.max(0.0).round() as u32),
            bold: truthy(osf_get(a, "bold")),
            italic: truthy(osf_get(a, "italic")),
            underline: truthy(osf_get(a, "underline")),
            all_caps: truthy(osf_get(a, "allcaps")),
            hidden: false,
            align: osf_get(a, "align").and_then(align_from),
            left: length("leftindent").map(|l| margin_left + l),
            right: length("rightindent").map(|r| paper_right - r),
            first: length("firstlineindent"),
            space_before: osf_get(a, "spacebefore").and_then(number).map(|v| (v.max(0.0) * 12.0).round() as u32),
            spacing: osf_get(a, "linespacing").and_then(hundredths),
            new_page: truthy(osf_get(a, "pagebreakbefore")),
            enter: osf_get(a, "style_enter").map(str::to_string),
            tab: osf_get(a, "style_tab_after").map(str::to_string),
            fdx_xml: osf_get(a, FDX_ATTR).unwrap_or_default().to_string(),
            osf_xml: xml.to_string(),
        });
    }
    for element in &mut format.elements {
        let Some(fdx) = Node::parse(&element.fdx_xml).map(|n| element_from_fdx(&n, "")) else { continue };
        // Compared as OSF writes them: from the margins.
        refine_by(&mut element.left, fdx.left, |l| l - margin_left);
        refine_by(&mut element.right, fdx.right, |r| paper_right - r);
        refine(&mut element.first, fdx.first);
        // OSF has no hidden text.
        element.hidden = fdx.hidden;
    }
    format
}

/// `value` replaced by `finer` when the two are the same length in OSF's tenths of a millimetre.
fn refine(value: &mut Option<Mils>, finer: Option<Mils>) {
    refine_by(value, finer, |v| v);
}

/// [`refine`], comparing the lengths as `osf` turns them into what OSF writes.
fn refine_by(value: &mut Option<Mils>, finer: Option<Mils>, osf: impl Fn(Mils) -> Mils) {
    if let (Some(v), Some(f)) = (*value, finer)
        && tenth_mm(osf(v)) == tenth_mm(osf(f))
    {
        *value = Some(f);
    }
}

/// OSF's built-in styles and their numbers.
const BUILTIN: [&str; 8] = ["Normal Text", "Scene Heading", "Action", "Character", "Parenthetical", "Dialogue", "Transition", "Shot"];

/// A style in the spelling of a document's version.
pub(crate) struct OsfSpelling {
    pub base_style: &'static str,
}

/// An element's OSF `<style>`, from the one it came with brought up to date.
pub(crate) fn osf_style(element: &ElementFormat, page: Option<&PageFormat>, spelling: &OsfSpelling) -> String {
    let page = page.cloned().unwrap_or_default();
    let margin_left = page.left.unwrap_or(OSF_LEFT);
    let paper_right = page.width.unwrap_or(8500) - page.right.unwrap_or(OSF_RIGHT);
    let fresh = element.osf_xml.is_empty();
    let mut node = Node::parse(&element.osf_xml).unwrap_or_else(|| Node::new("style"));
    let set = |node: &mut Node, name: &str, value: Option<String>, means: &dyn Fn(&str) -> bool| {
        let key = node.attributes.iter().find(|(k, _)| loose(k) == loose(name)).map(|(k, _)| k.clone()).unwrap_or_else(|| name.to_string());
        node.set(&key, value, means);
    };
    set(&mut node, "name", Some(element.name.clone()), &|v| v == element.name);
    if fresh && let Some(index) = BUILTIN.iter().position(|b| *b == element.name) {
        set(&mut node, "builtin", Some("1".into()), &|v| v == "1");
        set(&mut node, "builtin_index", Some(index.to_string()), &|_| true);
    }
    let base_key = node.attributes.iter().find(|(k, _)| matches!(loose(k).as_str(), "basestylename" | "basestyle")).map(|(k, _)| k.clone());
    node.set(base_key.as_deref().unwrap_or(spelling.base_style), element.based_on.clone(), |v| Some(v) == element.based_on.as_deref());
    set(&mut node, "font", element.font.clone(), &|v| Some(v) == element.font.as_deref());
    set(&mut node, "size", element.size.map(|s| s.to_string()), &|v| number(v).map(|n| n.round() as u32) == element.size);
    for (name, on) in [("bold", element.bold), ("italic", element.italic), ("underline", element.underline), ("allcaps", element.all_caps)] {
        let present = osf_get(&node.attributes, name).is_some();
        set(&mut node, name, (on || present).then(|| if on { "1" } else { "0" }.to_string()), &|v| truthy(Some(v)) == on);
    }
    let space = element.space_before;
    set(&mut node, "spacebefore", space.map(|s| format!("{:.1}", f64::from(s) / 12.0)), &|v| number(v).map(|n| (n * 12.0).round() as u32) == space);
    set(&mut node, "align", element.align.map(|a| align_name(a, false).to_string()), &|v| align_from(v) == element.align);
    let left = element.left.map(|l| l - margin_left);
    set(&mut node, "leftindent", left.map(tenth_mm), &|v| mils_from_tenth_mm(v) == left);
    let first = element.first;
    set(&mut node, "firstlineindent", first.map(tenth_mm), &|v| mils_from_tenth_mm(v) == first);
    let right = element.right.map(|r| paper_right - r);
    set(&mut node, "rightindent", right.map(tenth_mm), &|v| mils_from_tenth_mm(v) == right);
    set(&mut node, "linespacing", element.spacing.map(|s| format!("{:.1}", f64::from(s) / 100.0)), &|v| hundredths(v) == element.spacing);
    let present = osf_get(&node.attributes, "pagebreakbefore").is_some();
    set(&mut node, "pagebreakbefore", (element.new_page || present).then(|| if element.new_page { "1" } else { "0" }.to_string()), &|v| {
        truthy(Some(v)) == element.new_page
    });
    set(&mut node, "style_enter", element.enter.clone(), &|v| Some(v) == element.enter.as_deref());
    set(&mut node, "style_tab_after", element.tab.clone(), &|v| Some(v) == element.tab.as_deref());
    let carried = Some(element.fdx_xml.clone()).filter(|_| fdx_needed(element, Some(&page)));
    node.set(FDX_ATTR, carried, |v| v == element.fdx_xml);
    node.to_xml()
}

/// The OSF `settings` tag, from the one the page came with (or `base`) brought up to date.
pub(crate) fn osf_settings(page: &PageFormat, base: &str) -> String {
    let source = if page.osf_settings.is_empty() { base } else { &page.osf_settings };
    let mut node = Node::parse(source).unwrap_or_else(|| Node::new("settings"));
    let set = |node: &mut Node, name: &str, value: Option<String>, means: &dyn Fn(&str) -> bool| {
        let key = node.attributes.iter().find(|(k, _)| loose(k) == loose(name)).map(|(k, _)| k.clone()).unwrap_or_else(|| name.to_string());
        node.set(&key, value, means);
    };
    for (name, value) in [
        ("page_width", page.width),
        ("page_height", page.height),
        ("margin_top", page.top),
        ("margin_bottom", page.bottom),
        ("margin_left", page.left),
        ("margin_right", page.right),
    ] {
        // A value the page does not say is left as it was.
        if value.is_some() {
            set(&mut node, name, value.map(tenth_mm), &|v| mils_from_tenth_mm(v) == value);
        }
    }
    if let Some(more) = &page.more {
        set(&mut node, "more_text", Some(more.clone()), &|v| v == more);
    }
    if let Some(contd) = &page.contd {
        set(&mut node, "cont_text", Some(contd.clone()), &|v| bracketed(v) == bracketed(contd));
    }
    if let Some(auto) = page.auto_contd {
        set(&mut node, "dialogue_continues", Some(auto.to_string()), &|v| truthy(Some(v)) == auto);
    }
    let bare = PageFormat { fdx_page_layout: String::new(), fdx_mores: String::new(), ..page.clone() };
    let layout = Some(page.fdx_page_layout.clone()).filter(|x| !x.is_empty() && *x != fdx_page_layout(&bare));
    node.set("fdx_page_layout", layout, |v| v == page.fdx_page_layout);
    let mores = Some(page.fdx_mores.clone()).filter(|x| !x.is_empty() && *x != fdx_mores(&bare));
    node.set("fdx_mores", mores, |v| v == page.fdx_mores);
    node.to_xml()
}

// ---------------------------------------------------------------------------------------------
// Layout

/// Mils to character columns (10 an inch).
fn columns(mils: Mils) -> u16 {
    (f64::from(mils.max(0)) / 100.0).round() as u16
}

/// Applies a script's format to `layout`: page size and margins, mores and continueds, and the
/// built-in and custom element styles.
pub fn apply(format: &Format, layout: &mut Layout) {
    let page = format.page.clone().unwrap_or_default();
    if let Some(page) = &format.page {
        let (width, height) = (page.width.unwrap_or(8500), page.height.unwrap_or(11000));
        if (5000..=20000).contains(&height) && (4000..=15000).contains(&width) {
            layout.page_rows = (f64::from(height) * 6.0 / 1000.0).round() as u16;
            layout.page_columns = (f64::from(width) / 100.0).round() as u16;
        } else if (5000..=20000).contains(&height) {
            layout.page_rows = (f64::from(height) * 6.0 / 1000.0).round() as u16;
        }
        let rows = |mils: Mils| (f64::from(mils.clamp(0, 10000)) * 6.0 / 1000.0).round() as u16;
        let top = page.top.map_or(6, rows);
        let bottom = page.bottom.map_or(6, rows);
        if page.top.is_some() || page.height.is_some() {
            layout.body_top = top;
            layout.lines_per_page = layout.page_rows.saturating_sub(top).saturating_sub(bottom).max(10);
        }
        if let Some(more) = page.more.as_deref().filter(|m| !m.trim().is_empty()) {
            layout.more = more.to_string();
        }
        if let Some(contd) = page.contd.as_deref().filter(|c| !c.trim().is_empty()) {
            layout.contd = contd.to_string();
        }
        if let Some(auto) = page.auto_contd {
            layout.auto_contd = auto;
        }
    }
    let margin_left = page.left.unwrap_or(OSF_LEFT);
    let paper_right = page.width.unwrap_or(8500) - page.right.unwrap_or(OSF_RIGHT);
    for element in &format.elements {
        let apply_to = |style: &mut ElementStyle| {
            if element.left.is_some() || element.right.is_some() || page.left.is_some() {
                let left = element.left.unwrap_or(margin_left);
                let right = element.right.unwrap_or(paper_right);
                if right > left {
                    style.left = columns(left + element.first.unwrap_or(0).min(0));
                    style.width = chars_per_line((f64::from(right - left) / 1000.0) as f32);
                }
            }
            if let Some(space) = element.space_before {
                style.space_before = (f64::from(space.min(720)) / 12.0).round() as u16;
            }
            if let Some(align) = element.align {
                style.align = align;
            }
        };
        let target = match element.name.to_ascii_lowercase().as_str() {
            "scene heading" => Some(&mut layout.scene_heading),
            "action" => Some(&mut layout.action),
            "character" => Some(&mut layout.character),
            "parenthetical" => Some(&mut layout.parenthetical),
            "dialogue" => Some(&mut layout.dialogue),
            "transition" => Some(&mut layout.transition),
            "shot" => Some(&mut layout.shot),
            "lyrics" => Some(&mut layout.lyrics),
            _ => None,
        };
        match target {
            Some(style) => apply_to(style),
            None if !element.name.is_empty() => {
                let mut style = layout.action;
                apply_to(&mut style);
                layout.custom.retain(|(name, _)| *name != element.name);
                layout.custom.push((element.name.clone(), style));
            }
            None => {}
        }
    }
}
