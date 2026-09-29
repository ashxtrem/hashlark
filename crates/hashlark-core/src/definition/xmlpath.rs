// SPDX-License-Identifier: GPL-3.0-or-later

//! A small path language for XML (RSS, Atom, Torznab), a tiny subset of
//! XPath:
//!
//! - `channel/item` — child elements, step by step
//! - `//item` — any descendant named `item`
//! - `enclosure/@url` — an attribute of the selected element
//! - `torznab:attr[@name='seeders']/@value` — a predicate on an attribute
//! - `.` — the element itself; `*` matches any element name
//!
//! Names may carry the prefix used in the document (`torznab:attr`); a name
//! without a prefix matches regardless of namespace.

use roxmltree::Node;

#[derive(Debug, Clone, PartialEq)]
pub struct XmlPath {
    descendant: bool,
    steps: Vec<Step>,
    attr: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct Step {
    name: String,
    predicate: Option<(String, String)>,
}

impl XmlPath {
    pub fn parse(path: &str) -> Result<Self, String> {
        let path = path.trim();
        if path.is_empty() {
            return Err("empty XML path".into());
        }
        let (descendant, rest) = match path.strip_prefix("//") {
            Some(rest) => (true, rest),
            None => (false, path.trim_start_matches('/')),
        };
        let mut steps = Vec::new();
        let mut attr = None;
        let parts: Vec<&str> = rest.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            let part = part.trim();
            if let Some(name) = part.strip_prefix('@') {
                if i != parts.len() - 1 || name.is_empty() {
                    return Err(format!("`@attribute` must be the last step in `{path}`"));
                }
                attr = Some(name.to_owned());
            } else if part == "." {
                continue;
            } else {
                steps.push(Step::parse(part, path)?);
            }
        }
        if descendant && steps.is_empty() {
            return Err(format!(
                "`//` must be followed by an element name in `{path}`"
            ));
        }
        Ok(Self {
            descendant,
            steps,
            attr,
        })
    }

    /// Elements selected from `node`.
    pub fn select<'a, 'input>(&self, node: Node<'a, 'input>) -> Vec<Node<'a, 'input>> {
        let mut current: Vec<Node<'a, 'input>> = vec![node];
        let mut steps = self.steps.iter();
        if self.descendant {
            let first = steps.next().expect("checked in parse");
            current = node
                .descendants()
                .filter(|n| n.is_element() && first.matches(*n))
                .collect();
        }
        for step in steps {
            current = current
                .iter()
                .flat_map(|n| n.children().filter(|c| c.is_element() && step.matches(*c)))
                .collect();
        }
        current
    }

    /// Selects from the document, also trying from the root element so that
    /// both `rss/channel/item` and `channel/item` work.
    pub fn select_from_document<'a, 'input>(
        &self,
        doc: &'a roxmltree::Document<'input>,
    ) -> Vec<Node<'a, 'input>> {
        let found = self.select(doc.root());
        if found.is_empty() && !self.descendant {
            self.select(doc.root_element())
        } else {
            found
        }
    }

    /// The first selected value: the attribute if the path ends in `@name`,
    /// otherwise the element's text.
    pub fn value(&self, node: Node<'_, '_>) -> Option<String> {
        let target = self.select(node).into_iter().next()?;
        match &self.attr {
            Some(name) => attribute(target, name).map(str::to_owned),
            None => Some(text_of(target)),
        }
    }
}

impl Step {
    fn parse(part: &str, path: &str) -> Result<Self, String> {
        let (name, predicate) = match part.split_once('[') {
            None => (part, None),
            Some((name, rest)) => {
                let inner = rest
                    .strip_suffix(']')
                    .ok_or_else(|| format!("unclosed `[` in `{path}`"))?;
                let (key, value) = inner
                    .trim()
                    .strip_prefix('@')
                    .and_then(|p| p.split_once('='))
                    .ok_or_else(|| {
                        format!("predicates must look like [@name='value'] in `{path}`")
                    })?;
                let value = value.trim().trim_matches(|c| c == '\'' || c == '"');
                (name, Some((key.trim().to_owned(), value.to_owned())))
            }
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("empty step in `{path}`"));
        }
        Ok(Self {
            name: name.to_owned(),
            predicate,
        })
    }

    fn matches(&self, node: Node<'_, '_>) -> bool {
        let name_ok = self.name == "*" || {
            let local = node.tag_name().name();
            match self.name.split_once(':') {
                None => self.name == local,
                Some((prefix, wanted_local)) => {
                    wanted_local == local
                        && node
                            .tag_name()
                            .namespace()
                            .and_then(|ns| node.lookup_prefix(ns))
                            == Some(prefix)
                }
            }
        };
        name_ok
            && self
                .predicate
                .as_ref()
                .is_none_or(|(key, value)| attribute(node, key) == Some(value.as_str()))
    }
}

fn attribute<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    match name.split_once(':') {
        None => node.attribute(name),
        Some((prefix, local)) => {
            let ns = node.lookup_namespace_uri(Some(prefix))?;
            node.attribute((ns, local))
        }
    }
}

/// All text inside `node`, with whitespace collapsed.
pub fn text_of(node: Node<'_, '_>) -> String {
    let raw: String = node
        .descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0"?>
<rss xmlns:torznab="http://torznab.com/schemas/2015/feed">
  <channel>
    <item>
      <title>First &amp; best</title>
      <enclosure url="https://x.example/1.torrent" length="10"/>
      <torznab:attr name="seeders" value="12"/>
      <torznab:attr name="peers" value="30"/>
      <description><![CDATA[Some <b>html</b>]]></description>
    </item>
    <item><title>Second</title></item>
  </channel>
</rss>"#;

    fn doc() -> roxmltree::Document<'static> {
        roxmltree::Document::parse(RSS).unwrap()
    }

    #[test]
    fn selects_rows_in_several_ways() {
        let doc = doc();
        for path in [
            "//item",
            "rss/channel/item",
            "channel/item",
            "/rss/channel/item",
        ] {
            let rows = XmlPath::parse(path).unwrap().select_from_document(&doc);
            assert_eq!(rows.len(), 2, "{path}");
        }
    }

    #[test]
    fn reads_text_attributes_and_predicates() {
        let doc = doc();
        let item = XmlPath::parse("//item").unwrap().select_from_document(&doc)[0];
        let get = |p: &str| XmlPath::parse(p).unwrap().value(item);
        assert_eq!(get("title").as_deref(), Some("First & best"));
        assert_eq!(
            get("enclosure/@url").as_deref(),
            Some("https://x.example/1.torrent")
        );
        assert_eq!(
            get("torznab:attr[@name='seeders']/@value").as_deref(),
            Some("12")
        );
        assert_eq!(get("attr[@name=\"peers\"]/@value").as_deref(), Some("30"));
        assert_eq!(get("description").as_deref(), Some("Some <b>html</b>"));
        assert_eq!(get("missing"), None);
        assert_eq!(get("other:attr[@name='seeders']/@value"), None);
    }

    #[test]
    fn rejects_bad_paths() {
        for bad in ["", "//", "a/@b/c", "a[@x", "a[x=1]", "a//"] {
            assert!(XmlPath::parse(bad).is_err(), "{bad}");
        }
    }
}
