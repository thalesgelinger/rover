use crate::component::HtmlPatch;
use regex::Regex;
use std::collections::{HashSet, HashMap};

/// Generate patches between old and new HTML
/// Returns None if the change is too complex (should send full HTML instead)
pub fn diff_html(old_html: &str, new_html: &str) -> Option<Vec<HtmlPatch>> {
    // If HTML is identical, no patches needed
    if old_html == new_html {
        return Some(Vec::new());
    }

    // If change is too large, use full HTML
    if new_html.len() > 5120 || old_html.is_empty() {
        return None;
    }

    let mut patches = Vec::new();
    let mut seen_selectors = HashSet::new();

    // Extract all elements with data-rover-id from both versions
    let old_elements = extract_elements(old_html);
    let new_elements = extract_elements(new_html);

    // Convert to HashMap for O(1) lookups
    let old_map: HashMap<String, HtmlElement> = old_elements.into_iter().collect();

    // For each element with ID, compare old vs new
    for (selector, new_element) in &new_elements {
        if let Some(old_element) = old_map.get(selector) {
            // Element exists in both - check for changes
            if let Some(patch) = diff_element(selector, old_element, new_element) {
                if !seen_selectors.contains(selector) {
                    patches.push(patch);
                    seen_selectors.insert(selector.to_string());
                }
            }
        }
    }

    // If too many patches, use full HTML instead
    if patches.len() > 10 {
        return None;
    }

    Some(patches)
}

/// HTML element representation
#[derive(Debug, Clone)]
struct HtmlElement {
    tag: String,
    inner_html: String,
    attributes: Vec<(String, String)>,
}

/// Extract all elements with data-rover-id from HTML
fn extract_elements(html: &str) -> Vec<(String, HtmlElement)> {
    let mut elements = Vec::new();

    // Find opening tags with data-rover-id attribute
    // Pattern: <tag ... data-rover-id="id" ...>
    let tag_re = Regex::new(r#"<([a-zA-Z][a-zA-Z0-9]*)\s[^>]*data-rover-id="([^"]+)"[^>]*>"#).unwrap();

    for cap in tag_re.captures_iter(html) {
        let tag = cap.get(1).unwrap().as_str().to_string();
        let id = cap.get(2).unwrap().as_str().to_string();
        let full_match = cap.get(0).unwrap();
        let start_pos = full_match.end();

        // Find the closing tag for this element
        let closing_tag = format!("</{}>", tag);
        if let Some(rel_end) = html[start_pos..].find(&closing_tag) {
            let inner_html = html[start_pos..start_pos + rel_end].to_string();

            // Extract all attributes from the opening tag
            let attrs = extract_attributes(full_match.as_str());

            elements.push((format!("[data-rover-id=\"{}\"]", id), HtmlElement {
                tag,
                inner_html,
                attributes: attrs,
            }));
        }
    }

    elements
}

/// Extract all attributes from an HTML element opening tag
fn extract_attributes(element_str: &str) -> Vec<(String, String)> {
    let mut attributes = Vec::new();

    // Extract opening tag: <tag ...>
    if let Some(start) = element_str.find('<') {
        if let Some(end) = element_str.find('>') {
            let tag_open = &element_str[start + 1..end];

            // Skip the tag name, get just the attributes part
            let attrs_part = tag_open.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");

            // Parse attributes: name="value" or name='value'
            let attr_re = Regex::new(r#"([a-zA-Z-]+)="([^"]*)"|'([^']*)'"#).unwrap();
            for cap in attr_re.captures_iter(&attrs_part) {
                if let (Some(name), Some(value)) = (cap.get(1), cap.get(2)) {
                    attributes.push((name.as_str().to_string(), value.as_str().to_string()));
                }
            }
        }
    }

    attributes
}

/// Compare two HTML elements and generate a patch if different
fn diff_element(selector: &str, old: &HtmlElement, new: &HtmlElement) -> Option<HtmlPatch> {
    // Check if inner HTML changed
    if old.inner_html != new.inner_html {
        // If inner HTML is simple text (no HTML tags), use ReplaceText
        if !old.inner_html.contains('<') && !new.inner_html.contains('<') {
            return Some(HtmlPatch::ReplaceText {
                selector: selector.to_string(),
                text: new.inner_html.clone(),
            });
        } else {
            // Complex inner HTML change - use ReplaceInnerHTML
            return Some(HtmlPatch::ReplaceInnerHTML {
                selector: selector.to_string(),
                html: new.inner_html.clone(),
            });
        }
    }

    // Check for attribute changes
    let old_attrs: std::collections::HashMap<String, String> = old.attributes.iter().cloned().collect();
    let new_attrs: std::collections::HashMap<String, String> = new.attributes.iter().cloned().collect();

    // Find added/changed attributes
    for (name, new_value) in &new_attrs {
        if let Some(old_value) = old_attrs.get(name) {
            if old_value != new_value {
                return Some(HtmlPatch::SetAttribute {
                    selector: selector.to_string(),
                    attr: name.clone(),
                    value: new_value.clone(),
                });
            }
        } else if name != "data-rover-id" {
            // Skip data-rover-id (we don't need to set it)
            return Some(HtmlPatch::SetAttribute {
                selector: selector.to_string(),
                attr: name.clone(),
                value: new_value.clone(),
            });
        }
    }

    // Find removed attributes
    for (name, _) in &old_attrs {
        if !new_attrs.contains_key(name) {
            return Some(HtmlPatch::RemoveAttribute {
                selector: selector.to_string(),
                attr: name.clone(),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_identical() {
        let html = r#"<div data-rover-id="123">Hello</div>"#;
        let patches = diff_html(html, html);
        assert!(patches.is_some());
        assert!(patches.unwrap().is_empty());
    }

    #[test]
    fn test_diff_text_change() {
        let old = r#"<div data-rover-id="123">Hello</div>"#;
        let new = r#"<div data-rover-id="123">World</div>"#;
        let patches = diff_html(old, new);
        assert!(patches.is_some());
        let patches = patches.unwrap();
        assert_eq!(patches.len(), 1);
        match &patches[0] {
            HtmlPatch::ReplaceText { selector, text } => {
                assert!(selector.contains("123"));
                assert_eq!(text, "World");
            }
            _ => panic!("Expected ReplaceText patch"),
        }
    }

    #[test]
    fn test_diff_attribute_change() {
        let old = r#"<input type="checkbox" data-rover-id="123" checked>"#;
        let new = r#"<input type="checkbox" data-rover-id="123">"#;
        let patches = diff_html(old, new);
        assert!(patches.is_some());
        let patches = patches.unwrap();
        assert_eq!(patches.len(), 1);
        match &patches[0] {
            HtmlPatch::RemoveAttribute { selector, attr } => {
                assert!(selector.contains("123"));
                assert_eq!(attr, "checked");
            }
            _ => panic!("Expected RemoveAttribute patch"),
        }
    }

    #[test]
    fn test_diff_too_large() {
        let old = "";
        let new = "x".repeat(6000);
        let patches = diff_html(old, &new);
        assert!(patches.is_none());
    }

    #[test]
    fn test_diff_too_many_changes() {
        let mut old = String::new();
        let mut new = String::new();
        for i in 0..15 {
            old.push_str(&format!(r#"<div data-rover-id="{}">{}</div>"#, i, i));
            new.push_str(&format!(r#"<div data-rover-id="{}">{}</div>"#, i, i + 1));
        }
        let patches = diff_html(&old, &new);
        assert!(patches.is_none());
    }

    #[test]
    fn test_extract_elements() {
        let html = r#"
            <div data-rover-id="1">Item 1</div>
            <span data-rover-id="2">Item 2</span>
        "#;
        let elements = extract_elements(html);
        assert_eq!(elements.len(), 2);
    }
}
