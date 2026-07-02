use std::collections::BTreeMap;

pub(crate) fn parse(source: &str) -> (BTreeMap<String, String>, &str) {
    let opening_end = if source.starts_with("---\r\n") {
        5
    } else if source.starts_with("---\n") {
        4
    } else {
        return (BTreeMap::new(), source);
    };
    let mut closing = None;
    let mut offset = opening_end;
    for segment in source[opening_end..].split_inclusive('\n') {
        if segment.trim_end_matches(['\r', '\n']) == "---" {
            closing = Some((offset, offset + segment.len()));
            break;
        }
        offset += segment.len();
    }
    let Some((header_end, body_start)) = closing else {
        return (BTreeMap::new(), source);
    };
    let header = &source[opening_end..header_end];
    let body = &source[body_start..];
    let mut values = BTreeMap::new();
    let mut current_key = None::<String>;
    let mut current_value = String::new();
    for line in header.lines() {
        let top_level = !line.starts_with(' ') && !line.starts_with('\t');
        if top_level && line.contains(':') {
            if let Some(key) = current_key.take() {
                values.insert(key, normalize_value(&current_value));
                current_value.clear();
            }
            if let Some((key, value)) = line.split_once(':') {
                current_key = Some(key.trim().to_string());
                current_value.push_str(value.trim());
            }
        } else if current_key.is_some() {
            current_value.push('\n');
            current_value.push_str(line);
        }
    }
    if let Some(key) = current_key {
        values.insert(key, normalize_value(&current_value));
    }
    (values, body)
}

fn normalize_value(value: &str) -> String {
    let value = value.trim();
    if !value.contains('\n') {
        value.trim_matches('"').to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn first_h1(body: &str) -> Option<String> {
    body.lines().find_map(|line| {
        line.strip_prefix("# ")
            .map(|title| title.trim().to_string())
    })
}
