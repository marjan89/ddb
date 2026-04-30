use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Element {
    pub x: i32,
    pub y: i32,
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub clickable: bool,
}

struct RawNode {
    text: String,
    desc: String,
    resource_id: String,
    clickable: bool,
    bounds: String,
}

pub fn parse(xml: &str) -> Vec<Element> {
    let mut reader = Reader::from_str(xml);
    let mut raw_nodes = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) if e.name().as_ref() == b"node" => {
                if let Some(node) = parse_node(e) {
                    raw_nodes.push(node);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    dedup(raw_nodes)
}

fn parse_node(e: &quick_xml::events::BytesStart) -> Option<RawNode> {
    let mut text = String::new();
    let mut desc = String::new();
    let mut resource_id = String::new();
    let mut clickable = false;
    let mut focusable = false;
    let mut bounds = String::new();

    for attr in e.attributes().flatten() {
        let val = String::from_utf8_lossy(&attr.value).into_owned();
        match attr.key.as_ref() {
            b"text" => text = val,
            b"content-desc" => desc = val,
            b"resource-id" => resource_id = val,
            b"clickable" => clickable = val == "true",
            b"focusable" => focusable = val == "true",
            b"bounds" => bounds = val,
            _ => {}
        }
    }

    let label = if !text.is_empty() {
        &text
    } else if !desc.is_empty() {
        &desc
    } else {
        ""
    };
    let sid = short_id(&resource_id);

    // Skip pure containers
    if label.is_empty() && !(clickable || focusable) {
        return None;
    }
    if label.is_empty() && sid.is_empty() {
        return None;
    }
    if bounds.is_empty() {
        return None;
    }

    Some(RawNode {
        text,
        desc,
        resource_id,
        clickable,
        bounds,
    })
}

fn dedup(nodes: Vec<RawNode>) -> Vec<Element> {
    let mut elements: Vec<(Element, bool)> = Vec::new(); // (element, has_text)
    let mut seen_bounds = std::collections::HashSet::new();

    for node in &nodes {
        if !seen_bounds.insert(&node.bounds) {
            continue;
        }

        let (cx, cy) = match center(&node.bounds) {
            Some(c) => c,
            None => continue,
        };

        let sid = short_id(&node.resource_id);
        let label_text = if !node.text.is_empty() {
            &node.text
        } else if !node.desc.is_empty() {
            &node.desc
        } else {
            ""
        };
        let display_label = if !label_text.is_empty() {
            label_text.to_string()
        } else {
            sid.clone()
        };
        let has_text = !label_text.is_empty();

        let id_field = if !sid.is_empty() && sid != display_label {
            sid
        } else {
            String::new()
        };

        let elem = Element {
            x: cx,
            y: cy,
            label: display_label,
            id: id_field,
            clickable: node.clickable,
        };

        // Dedupe by proximity
        let nearby = elements
            .iter()
            .position(|(e, _)| (e.x - cx).abs() < 20 && (e.y - cy).abs() < 20);

        if let Some(idx) = nearby {
            let (ref existing, existing_has_text) = elements[idx];
            let prefer_new =
                (has_text && !existing_has_text) || (node.clickable && !existing.clickable);
            if prefer_new {
                elements[idx] = (elem, has_text);
            }
        } else {
            elements.push((elem, has_text));
        }
    }

    // Second pass: dedupe same label at same x bucket
    let mut seen_label_x: std::collections::HashMap<(String, i32), usize> =
        std::collections::HashMap::new();
    let mut result: Vec<Element> = Vec::new();

    for (elem, _) in elements {
        let key = (elem.label.clone(), elem.x / 30);
        if let Some(&idx) = seen_label_x.get(&key) {
            if !elem.id.is_empty() && result[idx].id.is_empty() {
                result[idx] = elem;
            }
        } else {
            seen_label_x.insert(key, result.len());
            result.push(elem);
        }
    }

    result
}

fn center(bounds: &str) -> Option<(i32, i32)> {
    // Bounds format: [x1,y1][x2,y2]
    let nums: Vec<i32> = bounds
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();

    if nums.len() < 4 {
        return None;
    }
    Some(((nums[0] + nums[2]) / 2, (nums[1] + nums[3]) / 2))
}

fn short_id(resource_id: &str) -> String {
    if let Some(pos) = resource_id.find(":id/") {
        resource_id[pos + 4..].to_string()
    } else {
        String::new()
    }
}
