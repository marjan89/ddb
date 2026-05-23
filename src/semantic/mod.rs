mod parser;
pub mod resource;
mod schema;

pub use schema::{Bounds, Font, Icon, Padding, SemanticElement, SemanticSchema};

use crate::adb;
use crate::registry::Device;

pub fn extract(
    dev: Option<&Device>,
    xml: &str,
    source_root: Option<&str>,
) -> Result<SemanticSchema, String> {
    let density = query_density(dev)?;
    let screen = query_activity(dev).unwrap_or_default();
    let device_name = dev
        .map(|d| d.model.clone())
        .unwrap_or_default();
    let raw_nodes = parser::parse_full(xml);

    let mut elements = Vec::new();
    let res_ctx = source_root.map(|root| resource::ResourceContext::load(root));

    for node in &raw_nodes {
        if let Some(elem) = build_element(node, density, res_ctx.as_ref()) {
            elements.push(elem);
        }
    }

    disambiguate_ids(&mut elements);

    Ok(SemanticSchema {
        screen,
        device: device_name,
        platform: "android".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        viewport: None,
        elements,
    })
}

fn query_activity(dev: Option<&Device>) -> Result<String, String> {
    let out = adb::shell(dev, &["dumpsys", "activity", "activities"])?;
    // Look for "mResumedActivity" or "topResumedActivity"
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.contains("mResumedActivity") || trimmed.contains("topResumedActivity") {
            // Extract "com.pkg/.ActivityName" pattern
            if let Some(start) = trimmed.find("u0 ") {
                let rest = &trimmed[start + 3..];
                if let Some(end) = rest.find(|c: char| c == ' ' || c == '}') {
                    let component = &rest[..end];
                    // Return just the activity short name
                    if let Some(slash) = component.rfind('/') {
                        let activity = &component[slash + 1..];
                        let activity = activity.strip_prefix('.').unwrap_or(activity);
                        return Ok(activity.to_string());
                    }
                    return Ok(component.to_string());
                }
            }
        }
    }
    Ok(String::new())
}

fn query_density(dev: Option<&Device>) -> Result<f64, String> {
    let out = adb::shell(dev, &["wm", "density"])?;
    // "Physical density: 450" or "Override density: 450"
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("Physical density:") {
            if let Ok(d) = rest.trim().parse::<f64>() {
                return Ok(d / 160.0);
            }
        }
        if let Some(rest) = line.strip_prefix("Override density:") {
            if let Ok(d) = rest.trim().parse::<f64>() {
                return Ok(d / 160.0);
            }
        }
    }
    Ok(2.5) // fallback: assume xhdpi-ish
}

fn build_element(
    node: &parser::FullNode,
    density: f64,
    res_ctx: Option<&resource::ResourceContext>,
) -> Option<SemanticElement> {
    let sid = short_id(&node.resource_id);

    let has_label = !node.text.is_empty() || !node.content_desc.is_empty();

    // Skip nodes with no text, no desc, not interactive, no resource-id
    if !has_label && !node.clickable && !node.focusable && sid.is_empty() {
        return None;
    }
    if !has_label && sid.is_empty() {
        return None;
    }

    // Skip structural containers (framework wrappers, navigation hosts, full-screen frames)
    if !has_label && !node.clickable {
        let dominated_by_framework = is_framework_container(&sid, &node.class_name);
        if dominated_by_framework {
            return None;
        }
    }

    let bounds = parse_bounds(&node.bounds, density)?;

    let content = if !node.text.is_empty() {
        Some(decode_entities(&node.text))
    } else if !node.content_desc.is_empty() {
        Some(decode_entities(&node.content_desc))
    } else {
        None
    };

    let elem_type = classify_type(&node.class_name, node.clickable, content.is_some());

    // Canonical ID: content-based slug first, resource-id as platform_id
    let platform_id = if !sid.is_empty() {
        Some(sid.clone())
    } else {
        None
    };

    let id = if let Some(ref c) = content {
        slugify(c)
    } else if !sid.is_empty() {
        to_snake(&sid)
    } else if !node.content_desc.is_empty() {
        slugify(&node.content_desc)
    } else {
        String::new()
    };

    let mut font: Option<Font> = None;
    let mut color: Option<String> = None;
    let mut background: Option<String> = None;
    let mut corner_radius: Option<f64> = None;
    let mut padding: Option<Padding> = None;
    let mut icon: Option<Icon> = None;

    if let Some(ctx) = res_ctx {
        if !node.resource_id.is_empty() {
            let short = short_id(&node.resource_id);
            if let Some(attrs) = ctx.resolve_view(&short) {
                font = attrs.font;
                color = attrs.text_color;
                background = attrs.background_color;
                corner_radius = attrs.corner_radius;
                padding = attrs.padding;
                icon = attrs.icon;
            }
        }
    }

    let a11y_id = platform_id.clone();
    Some(SemanticElement {
        id,
        platform_id,
        elem_type,
        content,
        font,
        color,
        bounds,
        clickable: node.clickable,
        enabled: node.enabled,
        accessible: Some(node.focusable || node.clickable || !node.content_desc.is_empty()),
        a11y_label: if !node.content_desc.is_empty() {
            Some(decode_entities(&node.content_desc))
        } else {
            None
        },
        a11y_id,
        background,
        corner_radius,
        padding,
        icon,
        margin: None,
        elevation: None,
        z_index: None,
        render: None,
        children: None,
    })
}

fn parse_bounds(bounds_str: &str, density: f64) -> Option<Bounds> {
    let nums: Vec<i32> = bounds_str
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();

    if nums.len() < 4 {
        return None;
    }

    Some(Bounds {
        x: (nums[0] as f64 / density).round() as i32,
        y: (nums[1] as f64 / density).round() as i32,
        w: ((nums[2] - nums[0]) as f64 / density).round() as i32,
        h: ((nums[3] - nums[1]) as f64 / density).round() as i32,
    })
}

fn classify_type(class_name: &str, clickable: bool, _has_text: bool) -> String {
    let short = class_name.rsplit('.').next().unwrap_or(class_name);
    match short {
        "TextView" if clickable => "button".to_string(),
        "TextView" => "text".to_string(),
        "EditText" => "input".to_string(),
        "ImageView" => "image".to_string(),
        "ImageButton" => "button".to_string(),
        "Button" => "button".to_string(),
        "CheckBox" | "RadioButton" | "Switch" | "ToggleButton" => "toggle".to_string(),
        "RecyclerView" | "ListView" | "ScrollView" => "list".to_string(),
        "HorizontalScrollView" => "scroll".to_string(),
        "ViewPager" => "pager".to_string(),
        "FrameLayout" | "LinearLayout" | "ViewGroup" => {
            if clickable {
                "button".to_string()
            } else {
                "container".to_string()
            }
        }
        _ => {
            if clickable {
                "button".to_string()
            } else {
                "view".to_string()
            }
        }
    }
}

fn disambiguate_ids(elements: &mut [SemanticElement]) {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for elem in elements.iter() {
        if !elem.id.is_empty() {
            *counts.entry(elem.id.clone()).or_insert(0) += 1;
        }
    }

    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for elem in elements.iter_mut() {
        if !elem.id.is_empty() {
            if let Some(&count) = counts.get(&elem.id) {
                if count > 1 {
                    let idx = seen.entry(elem.id.clone()).or_insert(0);
                    elem.id = format!("{}_{}", elem.id, *idx);
                    *idx += 1;
                }
            }
        }
    }
}

fn is_framework_container(short_id: &str, class_name: &str) -> bool {
    const FRAMEWORK_IDS: &[&str] = &[
        "action_bar_root",
        "content",
        "statusBarBackground",
        "navigationBarBackground",
    ];
    if FRAMEWORK_IDS.contains(&short_id) {
        return true;
    }
    // Navigation hosts, generic containers that span the screen
    if short_id.ends_with("NavigationHost") || short_id.ends_with("Container") {
        return true;
    }
    // ViewGroup/FrameLayout with an ID but no text — usually a wrapper
    let short_class = class_name.rsplit('.').next().unwrap_or(class_name);
    matches!(short_class, "FrameLayout" | "ViewGroup") && !short_id.is_empty()
}

fn short_id(resource_id: &str) -> String {
    if let Some(pos) = resource_id.find(":id/") {
        resource_id[pos + 4..].to_string()
    } else {
        String::new()
    }
}

fn slugify(s: &str) -> String {
    let decoded = decode_entities(s);
    let slug: String = decoded
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    // Collapse runs of underscores, trim edges
    slug.split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn to_snake(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}
