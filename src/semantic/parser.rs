use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug)]
pub struct FullNode {
    pub text: String,
    pub content_desc: String,
    pub resource_id: String,
    pub class_name: String,
    pub package: String,
    pub clickable: bool,
    pub focusable: bool,
    pub checkable: bool,
    pub checked: bool,
    pub enabled: bool,
    pub selected: bool,
    pub scrollable: bool,
    pub bounds: String,
    pub index: i32,
}

pub fn parse_full(xml: &str) -> Vec<FullNode> {
    let mut reader = Reader::from_str(xml);
    let mut nodes = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) if e.name().as_ref() == b"node" => {
                nodes.push(parse_full_node(e));
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    nodes
}

fn parse_full_node(e: &quick_xml::events::BytesStart) -> FullNode {
    let mut node = FullNode {
        text: String::new(),
        content_desc: String::new(),
        resource_id: String::new(),
        class_name: String::new(),
        package: String::new(),
        clickable: false,
        focusable: false,
        checkable: false,
        checked: false,
        enabled: true,
        selected: false,
        scrollable: false,
        bounds: String::new(),
        index: 0,
    };

    for attr in e.attributes().flatten() {
        let val = String::from_utf8_lossy(&attr.value).into_owned();
        match attr.key.as_ref() {
            b"text" => node.text = val,
            b"content-desc" => node.content_desc = val,
            b"resource-id" => node.resource_id = val,
            b"class" => node.class_name = val,
            b"package" => node.package = val,
            b"clickable" => node.clickable = val == "true",
            b"focusable" => node.focusable = val == "true",
            b"checkable" => node.checkable = val == "true",
            b"checked" => node.checked = val == "true",
            b"enabled" => node.enabled = val == "true",
            b"selected" => node.selected = val == "true",
            b"scrollable" => node.scrollable = val == "true",
            b"bounds" => node.bounds = val,
            b"index" => node.index = val.parse().unwrap_or(0),
            _ => {}
        }
    }

    node
}
