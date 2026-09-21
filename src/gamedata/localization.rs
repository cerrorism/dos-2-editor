//! Larian localization XML (`*.xml`) contentuid -> visible-text lookup.
use std::collections::HashMap;

pub fn parse_xml(xml: &str) -> HashMap<String, String> {
    let mut entries = HashMap::new();
    let mut remaining = xml;
    while let Some(attribute) = remaining.find("contentuid=\"") {
        let after_attribute = &remaining[attribute + "contentuid=\"".len()..];
        let Some(handle_end) = after_attribute.find('"') else {
            break;
        };
        let handle = &after_attribute[..handle_end];
        let after_handle = &after_attribute[handle_end + 1..];
        let Some(tag_end) = after_handle.find('>') else {
            break;
        };
        let content = &after_handle[tag_end + 1..];
        let Some(content_end) = content.find('<') else {
            break;
        };
        let text = decode_entities(content[..content_end].trim());
        if !handle.is_empty() && !text.is_empty() {
            entries.insert(handle.to_owned(), text);
        }
        remaining = &content[content_end + 1..];
    }
    entries
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_localization_entries_and_decodes_xml_entities() {
        let entries = parse_xml(r#"<content contentuid="h123">The &amp; &lt;Sword&gt;</content>"#);
        assert_eq!(entries.get("h123"), Some(&"The & <Sword>".to_owned()));
    }
}
