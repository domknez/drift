//! `drift shape`: what the services answer, as structure only — field names, types,
//! counts, and the values of a few fields that name kinds of things (thread types,
//! message types). Never names, ids, tokens or message text: for working out what
//! a tenant's answers look like without seeing what's in them.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Fields whose values are kinds, not content: they're shown.
const KINDS: &[&str] = &[
    "chatType",
    "threadType",
    "productThreadType",
    "messagetype",
    "messageType",
    "contenttype",
    "contentType",
    "isOneOnOne",
    "hidden",
    "isRead",
    "isGeneral",
    "isArchived",
    "type",
    "key",
    "uniquerosterthread",
    "size",
    "weight",
    "style",
    "isSubtle",
];

#[derive(Default)]
struct Node {
    seen: usize,
    types: BTreeSet<&'static str>,
    values: BTreeSet<String>,
    fields: BTreeMap<String, Node>,
    items: Option<Box<Node>>,
    longest: usize,
}

impl Node {
    fn add(&mut self, v: &Value, key: &str) {
        self.seen += 1;
        match v {
            Value::Null => {
                self.types.insert("null");
            }
            Value::Bool(b) => {
                self.types.insert("bool");
                if KINDS.contains(&key) {
                    self.values.insert(b.to_string());
                }
            }
            Value::Number(_) => {
                self.types.insert("number");
            }
            Value::String(s) => {
                self.types.insert("string");
                if KINDS.contains(&key) && s.len() < 60 {
                    self.values.insert(s.clone());
                } else if let Ok(inner @ (Value::Object(_) | Value::Array(_))) = serde_json::from_str::<Value>(s) {
                    // JSON sent as text: show its structure too.
                    self.types.insert("json-text");
                    self.items.get_or_insert_default().add(&inner, key);
                }
            }
            Value::Array(a) => {
                self.types.insert("array");
                self.longest = self.longest.max(a.len());
                let items = self.items.get_or_insert_default();
                for item in a.iter().take(400) {
                    items.add(item, key);
                }
            }
            Value::Object(o) => {
                self.types.insert("object");
                for (k, child) in o {
                    self.fields.entry(k.clone()).or_default().add(child, k);
                }
            }
        }
    }

    fn print(&self, out: &mut String, indent: usize, depth: usize) {
        let pad = "  ".repeat(indent);
        let types: Vec<&str> = self.types.iter().copied().collect();
        out.push_str(&format!("{} ×{}", types.join("|"), self.seen));
        if self.longest > 0 {
            out.push_str(&format!(" (up to {} items)", self.longest));
        }
        if !self.values.is_empty() {
            let values: Vec<&str> = self.values.iter().take(20).map(String::as_str).collect();
            out.push_str(&format!(" = {}", values.join(", ")));
        }
        out.push('\n');
        if depth == 0 {
            return;
        }
        for (k, child) in &self.fields {
            out.push_str(&format!("{pad}  {k}: "));
            child.print(out, indent + 1, depth - 1);
        }
        if let Some(items) = &self.items {
            out.push_str(&format!("{pad}  []: "));
            items.print(out, indent + 1, depth - 1);
        }
    }
}

/// The structure of `v`, `depth` levels deep.
pub fn describe(v: &Value, depth: usize) -> String {
    let mut root = Node::default();
    root.add(v, "");
    let mut out = String::new();
    root.print(&mut out, 0, depth);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_structure_but_not_content() {
        let v = serde_json::json!({"chats": [
            {"title": "Secret plans", "chatType": "chat", "members": [{"mri": "8:orgid:x"}]},
            {"title": "Other", "chatType": "meeting", "props": "{\"emotions\":[]}"}]});
        let text = describe(&v, 6);
        assert!(text.contains("chatType: string ×2 = chat, meeting"));
        assert!(text.contains("json-text"));
        assert!(!text.contains("Secret") && !text.contains("orgid"));
    }
}
