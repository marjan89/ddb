use std::collections::HashMap;
use std::path::Path;

use super::schema::{Font, Icon, Padding};

pub struct ResourceContext {
    colors: HashMap<String, String>,
    layouts: HashMap<String, LayoutAttrs>,
    drawables: HashMap<String, DrawableInfo>,
}

pub struct ViewAttrs {
    pub font: Option<Font>,
    pub text_color: Option<String>,
    pub background_color: Option<String>,
    pub corner_radius: Option<f64>,
    pub padding: Option<Padding>,
    pub icon: Option<Icon>,
}

struct LayoutAttrs {
    text_color: Option<String>,
    text_size: Option<f64>,
    font_family: Option<String>,
    background: Option<String>,
    padding: Option<[i32; 4]>,
    src: Option<String>,
}

enum DrawableInfo {
    Vector { name: String, paths: Vec<String> },
    Raster { name: String },
}

impl ResourceContext {
    pub fn load(source_root: &str) -> Self {
        let root = Path::new(source_root);
        let colors = load_colors(root);
        let layouts = scan_layouts(root);
        let drawables = scan_drawables(root);

        Self {
            colors,
            layouts,
            drawables,
        }
    }

    pub fn resolve_view(&self, view_id: &str) -> Option<ViewAttrs> {
        let layout = self.layouts.get(view_id)?;

        let text_color = layout
            .text_color
            .as_ref()
            .and_then(|c| self.resolve_color(c));

        let background_color = layout
            .background
            .as_ref()
            .and_then(|b| self.resolve_color(b));

        let font = match (&layout.font_family, layout.text_size) {
            (Some(family), Some(size)) => Some(parse_font(family, size)),
            (Some(family), None) => Some(Font {
                family: clean_font_family(family),
                weight: extract_weight(family),
                size: 0.0,
            }),
            (None, Some(size)) => Some(Font {
                family: String::new(),
                weight: String::new(),
                size,
            }),
            _ => None,
        };

        let icon = layout.src.as_ref().and_then(|s| self.resolve_drawable(s));

        Some(ViewAttrs {
            font,
            text_color,
            background_color,
            corner_radius: None,
            padding: layout.padding.map(|p| Padding {
                top: p[0],
                bottom: p[2],
                start: p[3],
                end: p[1],
            }),
            icon,
        })
    }

    fn resolve_color(&self, reference: &str) -> Option<String> {
        if reference.starts_with('#') {
            return Some(reference.to_string());
        }
        // @color/name or ?attr/name
        let name = reference
            .strip_prefix("@color/")
            .or_else(|| reference.strip_prefix("@android:color/"))?;
        self.colors.get(name).cloned()
    }

    fn resolve_drawable(&self, reference: &str) -> Option<Icon> {
        let name = reference
            .strip_prefix("@drawable/")
            .or_else(|| reference.strip_prefix("@mipmap/"))?;
        match self.drawables.get(name) {
            Some(DrawableInfo::Vector {
                name: n,
                paths: p,
            }) => Some(Icon {
                name: n.clone(),
                format: "vector".to_string(),
                paths: p.clone(),
            }),
            Some(DrawableInfo::Raster { name: n }) => Some(Icon {
                name: n.clone(),
                format: "raster".to_string(),
                paths: Vec::new(),
            }),
            None => None,
        }
    }
}

fn load_colors(root: &Path) -> HashMap<String, String> {
    let mut colors = HashMap::new();

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.file_name().map_or(true, |f| f != "colors.xml") {
            continue;
        }
        let parent = path.parent().and_then(|p| p.file_name());
        if !parent.map_or(false, |p| {
            let s = p.to_string_lossy();
            s == "values" || s.starts_with("values-")
        }) {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(path) {
            parse_color_xml(&content, &mut colors);
        }
    }

    colors
}

fn parse_color_xml(xml: &str, colors: &mut HashMap<String, String>) {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut current_name: Option<String> = None;
    let mut in_color = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"color" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"name" {
                        current_name =
                            Some(String::from_utf8_lossy(&attr.value).into_owned());
                        in_color = true;
                    }
                }
            }
            Ok(Event::Text(ref t)) if in_color => {
                if let Some(ref name) = current_name {
                    let val = t.unescape().map(|s| s.trim().to_string()).unwrap_or_default();
                    if val.starts_with('#') {
                        colors.insert(name.clone(), normalize_hex(&val));
                    }
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"color" => {
                in_color = false;
                current_name = None;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

fn normalize_hex(hex: &str) -> String {
    // #RGB → #RRGGBB, #ARGB → #AARRGGBB, pass through 7/9 char
    match hex.len() {
        4 => {
            let chars: Vec<char> = hex.chars().collect();
            format!(
                "#{}{}{}{}{}{}",
                chars[1], chars[1], chars[2], chars[2], chars[3], chars[3]
            )
        }
        5 => {
            let chars: Vec<char> = hex.chars().collect();
            format!(
                "#{}{}{}{}{}{}{}{}",
                chars[1], chars[1], chars[2], chars[2], chars[3], chars[3], chars[4], chars[4]
            )
        }
        _ => hex.to_uppercase(),
    }
}

fn scan_layouts(root: &Path) -> HashMap<String, LayoutAttrs> {
    let mut map = HashMap::new();

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "xml") {
            continue;
        }
        let parent = path.parent().and_then(|p| p.file_name());
        if !parent.map_or(false, |p| {
            let s = p.to_string_lossy();
            s == "layout" || s.starts_with("layout-")
        }) {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(path) {
            parse_layout_xml(&content, &mut map);
        }
    }

    map
}

fn parse_layout_xml(xml: &str, views: &mut HashMap<String, LayoutAttrs>) {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let mut id = String::new();
                let mut text_color = None;
                let mut text_size = None;
                let mut font_family = None;
                let mut background = None;
                let mut padding_vals: Option<[i32; 4]> = None;
                let mut src = None;

                let mut pad_all = None;
                let mut pad_top = None;
                let mut pad_bottom = None;
                let mut pad_start = None;
                let mut pad_end = None;

                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let val = String::from_utf8_lossy(&attr.value).into_owned();

                    // Strip android: prefix
                    let key = key
                        .strip_prefix("android:")
                        .or_else(|| key.strip_prefix("app:"))
                        .unwrap_or(&key);

                    match key {
                        "id" => {
                            id = val
                                .strip_prefix("@+id/")
                                .or_else(|| val.strip_prefix("@id/"))
                                .unwrap_or(&val)
                                .to_string();
                        }
                        "textColor" => text_color = Some(val),
                        "textSize" => text_size = parse_dp(&val),
                        "fontFamily" => font_family = Some(val),
                        "background" | "backgroundTint" => background = Some(val),
                        "src" | "srcCompat" => src = Some(val),
                        "padding" => pad_all = parse_dp_i32(&val),
                        "paddingTop" | "paddingVertical" => pad_top = parse_dp_i32(&val),
                        "paddingBottom" => pad_bottom = parse_dp_i32(&val),
                        "paddingStart" | "paddingLeft" | "paddingHorizontal" => {
                            pad_start = parse_dp_i32(&val)
                        }
                        "paddingEnd" | "paddingRight" => pad_end = parse_dp_i32(&val),
                        _ => {}
                    }
                }

                if !id.is_empty() {
                    if pad_all.is_some() || pad_top.is_some() || pad_start.is_some() {
                        let base = pad_all.unwrap_or(0);
                        padding_vals = Some([
                            pad_top.unwrap_or(base),
                            pad_end.unwrap_or(base),
                            pad_bottom.unwrap_or(base),
                            pad_start.unwrap_or(base),
                        ]);
                    }

                    views.insert(
                        id,
                        LayoutAttrs {
                            text_color,
                            text_size,
                            font_family,
                            background,
                            padding: padding_vals,
                            src,
                        },
                    );
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

fn scan_drawables(root: &Path) -> HashMap<String, DrawableInfo> {
    let mut map = HashMap::new();

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let parent = path.parent().and_then(|p| p.file_name());
        if !parent.map_or(false, |p| {
            let s = p.to_string_lossy();
            s == "drawable" || s.starts_with("drawable-")
        }) {
            continue;
        }

        let stem = match path.file_stem() {
            Some(s) => s.to_string_lossy().to_string(),
            None => continue,
        };

        match path.extension().and_then(|e| e.to_str()) {
            Some("xml") => {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Some(paths) = extract_vector_paths(&content) {
                        map.insert(
                            stem.clone(),
                            DrawableInfo::Vector {
                                name: stem,
                                paths,
                            },
                        );
                    }
                }
            }
            Some("png") | Some("jpg") | Some("webp") => {
                map.entry(stem.clone())
                    .or_insert(DrawableInfo::Raster { name: stem });
            }
            _ => {}
        }
    }

    map
}

fn extract_vector_paths(xml: &str) -> Option<Vec<String>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut paths = Vec::new();
    let mut is_vector = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag = e.name();
                if tag.as_ref() == b"vector" || tag.as_ref() == b"animated-vector" {
                    is_vector = true;
                }
                if tag.as_ref() == b"path" {
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        if key == "android:pathData" || key == "pathData" {
                            let val = String::from_utf8_lossy(&attr.value).into_owned();
                            if !val.is_empty() {
                                paths.push(val);
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    if is_vector && !paths.is_empty() {
        Some(paths)
    } else {
        None
    }
}

fn parse_dp(val: &str) -> Option<f64> {
    val.strip_suffix("dp")
        .or_else(|| val.strip_suffix("sp"))
        .or_else(|| val.strip_suffix("dip"))
        .and_then(|v| v.parse().ok())
        .or_else(|| val.parse().ok())
}

fn parse_dp_i32(val: &str) -> Option<i32> {
    parse_dp(val).map(|v| v as i32)
}

fn parse_font(family: &str, size: f64) -> Font {
    Font {
        family: clean_font_family(family),
        weight: extract_weight(family),
        size,
    }
}

fn clean_font_family(family: &str) -> String {
    // "@font/poppins_semibold" → "poppins"
    // "sans-serif-medium" → "sans-serif"
    let name = family
        .strip_prefix("@font/")
        .unwrap_or(family);

    let name = name
        .strip_prefix("res/font/")
        .unwrap_or(name);

    // Remove weight suffix
    for suffix in &[
        "_thin", "_extra_light", "_extralight", "_light", "_regular", "_medium",
        "_semi_bold", "_semibold", "_bold", "_extra_bold", "_extrabold", "_black",
        "-thin", "-light", "-regular", "-medium", "-semi-bold", "-semibold", "-bold",
    ] {
        if let Some(base) = name.strip_suffix(suffix) {
            return base.to_string();
        }
    }

    name.to_string()
}

fn extract_weight(family: &str) -> String {
    let lower = family.to_lowercase();
    if lower.contains("black") || lower.contains("900") {
        "black"
    } else if lower.contains("extrabold") || lower.contains("800") {
        "extrabold"
    } else if lower.contains("bold") || lower.contains("700") {
        "bold"
    } else if lower.contains("semibold") || lower.contains("semi_bold") || lower.contains("600") {
        "semibold"
    } else if lower.contains("medium") || lower.contains("500") {
        "medium"
    } else if lower.contains("regular") || lower.contains("400") {
        "regular"
    } else if lower.contains("light") {
        "light"
    } else if lower.contains("thin") {
        "thin"
    } else {
        "regular"
    }
    .to_string()
}
