// SPDX-License-Identifier: GPL-3.0-or-later

//! Converts Jackett's Cardigann definitions into Hashlark definitions (PLAN
//! §7.3). The conversion is best-effort: whatever has no equivalent is
//! listed in [`Conversion::warnings`] for a person to review.
//!
//! Jackett's definitions are GPL-2.0 licensed; Hashlark converts them only
//! on the user's machine and does not redistribute them.

use std::collections::BTreeMap;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use super::compile::{DefinitionError, compile};
use super::spec::{
    Caps, Definition, Download, FieldName, FieldSpec, FilterArgs, FilterSpec, HttpMethod, Login,
    LoginMethod, LoginTest, Mode, ResponseKind, Search, SettingDef, SettingKind, SiteType,
    TestSpec,
};
use crate::model::Category;

/// The outcome of a conversion.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Conversion {
    /// The Hashlark definition, as YAML.
    pub yaml: String,
    /// Things that couldn't be converted exactly; review them.
    pub warnings: Vec<String>,
    /// Whether the result passes validation as is.
    pub valid: bool,
    /// Validation problems, if not valid.
    pub errors: Vec<String>,
}

struct Converter {
    warnings: Vec<String>,
}

fn str_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object()?.get(key)
}

fn get_str(v: &Value, key: &str) -> Option<String> {
    get(v, key).and_then(str_of)
}

/// Maps a Newznab category name ("Movies/HD", "TV/Anime") to Hashlark's.
fn category_of_name(name: &str) -> Option<Category> {
    let lower = name.to_ascii_lowercase();
    let (top, sub) = lower.split_once('/').unwrap_or((&lower, ""));
    Some(match (top, sub) {
        ("tv", "anime") => Category::Anime,
        ("pc", "games") | ("pc", "mac") if sub == "games" => Category::Games,
        ("pc", "games") => Category::Games,
        ("movies", _) => Category::Movies,
        ("tv", _) => Category::Tv,
        ("audio", _) => Category::Music,
        ("pc", _) => Category::Software,
        ("console", _) => Category::Games,
        ("books", _) => Category::Books,
        ("xxx", _) | ("other", _) => Category::Other,
        _ => return None,
    })
}

impl Converter {
    fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    /// Converts a Go template (`{{ .Keywords }}`) to MiniJinja
    /// (`{{ query.text }}`).
    fn template(&mut self, input: &str, context: &str) -> String {
        let block = Regex::new(r"\{\{-?\s*(.*?)\s*-?\}\}").expect("valid regex");
        let mut out = String::new();
        let mut last = 0;
        for cap in block.captures_iter(input) {
            let whole = cap.get(0).expect("match");
            out.push_str(&input[last..whole.start()]);
            last = whole.end();
            let expr = cap[1].trim();
            // Conditions test values; only printed values get URL-encoded.
            let condition = format!("{context} (condition)");
            let converted = if let Some(cond) = expr.strip_prefix("if ") {
                format!("{{% if {} %}}", self.expr(cond, &condition))
            } else if let Some(cond) = expr.strip_prefix("else if ") {
                format!("{{% elif {} %}}", self.expr(cond, &condition))
            } else if expr == "else" {
                "{% else %}".to_owned()
            } else if expr == "end" {
                "{% endif %}".to_owned()
            } else if expr.starts_with("join .Categories") {
                let sep = expr.split('"').nth(1).unwrap_or(",");
                format!("{{{{ query.site_categories | join('{sep}') }}}}")
            } else {
                format!("{{{{ {} }}}}", self.expr(expr, context))
            };
            out.push_str(&converted);
        }
        out.push_str(&input[last..]);
        out
    }

    fn expr(&mut self, expr: &str, context: &str) -> String {
        let simple: &[(&str, &str)] = &[
            (".Keywords", "query.text"),
            (".Query.Keywords", "query.text"),
            (".Query.IMDBIDShort", "query.imdb | replace('tt', '')"),
            (".Query.IMDBID", "query.imdb"),
            (".Query.Page", "query.page"),
            (".Categories", "query.site_categories"),
        ];
        // In URL paths the words must be encoded; query parameters are
        // encoded when the request is built.
        if context == "search.path" && matches!(expr, ".Keywords" | ".Query.Keywords") {
            return "query.text | urlencode".to_owned();
        }
        for (from, to) in simple {
            if expr == *from {
                return (*to).to_owned();
            }
        }
        if let Some(name) = expr.strip_prefix(".Config.") {
            return format!("cfg.{name}");
        }
        if let Some(name) = expr.strip_prefix(".Result.") {
            if name.starts_with('_') {
                self.warn(format!(
                    "{context}: uses the helper field `{name}`, which has no equivalent; rewrite this template"
                ));
            }
            return format!("row.{name}");
        }
        if expr.starts_with('.') && !expr.contains(' ') {
            self.warn(format!("{context}: unknown template value `{expr}`"));
            return format!("\"\" {{# {expr} #}}");
        }
        self.warn(format!(
            "{context}: the template expression `{expr}` can't be converted; rewrite it"
        ));
        "\"\"".to_owned()
    }

    fn filters(&mut self, value: Option<&Value>, context: &str) -> Vec<FilterSpec> {
        let Some(Value::Array(list)) = value else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for filter in list {
            let Some(name) = get_str(filter, "name") else {
                continue;
            };
            let args = get(filter, "args");
            let arg = |i: usize| match args {
                Some(Value::Array(a)) => a.get(i).and_then(str_of),
                Some(v) if i == 0 => str_of(v),
                _ => None,
            };
            let one = |k: &str, v: Option<String>| {
                v.map(|v| {
                    FilterSpec::WithArgs(BTreeMap::from([(k.to_owned(), FilterArgs::One(v))]))
                })
            };
            let two = |k: &str, a: Option<String>, b: Option<String>| match (a, b) {
                (Some(a), Some(b)) => Some(FilterSpec::WithArgs(BTreeMap::from([(
                    k.to_owned(),
                    FilterArgs::Many(vec![a, b]),
                )]))),
                _ => None,
            };
            let converted = match name.as_str() {
                "trim" => Some(FilterSpec::Name("trim".into())),
                "tolower" => Some(FilterSpec::Name("lower".into())),
                "toupper" => Some(FilterSpec::Name("upper".into())),
                "timeago" | "fuzzytime" => Some(FilterSpec::Name("relative_date".into())),
                "dateparse" | "timeparse" => {
                    self.warn(format!(
                        "{context}: date format `{}` was replaced by automatic date detection; check dates parse",
                        arg(0).unwrap_or_default()
                    ));
                    Some(FilterSpec::Name("date".into()))
                }
                "regexp" => one("regex", arg(0)),
                "re_replace" => two("re_replace", arg(0), arg(1)),
                "replace" => two("replace", arg(0), arg(1)),
                "append" => one("append", arg(0)),
                "prepend" => one("prepend", arg(0)),
                "querystring" => one("querystring", arg(0)),
                "split" => two("split", arg(0), arg(1)),
                other => {
                    self.warn(format!(
                        "{context}: filter `{other}` isn't supported and was dropped"
                    ));
                    None
                }
            };
            out.extend(converted);
        }
        out
    }

    fn field(&mut self, spec: &Value, context: &str) -> FieldSpec {
        if get(spec, "remove").is_some() {
            self.warn(format!(
                "{context}: `remove` isn't supported; child elements' text is included"
            ));
        }
        FieldSpec {
            selector: get_str(spec, "selector"),
            path: None,
            attr: get_str(spec, "attribute"),
            text: get_str(spec, "text").map(|t| self.template(&t, context)),
            filters: self.filters(get(spec, "filters"), context),
            optional: get(spec, "optional")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    fn convert(&mut self, src: &Value) -> Definition {
        let raw_id = get_str(src, "id").unwrap_or_else(|| "imported".into());
        let id: String = raw_id
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_owned();
        if id != raw_id {
            self.warn(format!("id `{raw_id}` was changed to `{id}`"));
        }

        let site_type = match get_str(src, "type").as_deref() {
            Some("private") => SiteType::Private,
            Some("semi-private") => SiteType::SemiPrivate,
            _ => SiteType::Public,
        };
        let links: Vec<String> = match get(src, "links") {
            Some(Value::Array(a)) => a.iter().filter_map(str_of).collect(),
            _ => Vec::new(),
        };

        // Categories: first site id per Hashlark category.
        let mut categories = BTreeMap::new();
        if let Some(Value::Array(maps)) = get(src, "caps").and_then(|c| get(c, "categorymappings"))
        {
            for m in maps {
                let (Some(site), Some(cat)) = (get_str(m, "id"), get_str(m, "cat")) else {
                    continue;
                };
                match category_of_name(&cat) {
                    Some(c) => match categories.entry(c) {
                        std::collections::btree_map::Entry::Occupied(_) => self.warn(format!(
                            "site category {site} ({cat}) shares Hashlark category `{c}` with another; only the first is searched"
                        )),
                        std::collections::btree_map::Entry::Vacant(slot) => {
                            slot.insert(site);
                        }
                    },
                    None => self.warn(format!(
                        "site category {site} ({cat}) has no Hashlark equivalent"
                    )),
                }
            }
        }

        let settings = match get(src, "settings") {
            Some(Value::Array(list)) => list
                .iter()
                .filter_map(|s| {
                    let name = get_str(s, "name")?;
                    let kind = match get_str(s, "type").as_deref() {
                        Some("password") => SettingKind::Password,
                        Some("checkbox") => SettingKind::Checkbox,
                        Some("select") => SettingKind::Select,
                        Some("text") | None => SettingKind::Text,
                        Some(_) => return None, // "info" and friends are just notes
                    };
                    let options = get(s, "options")
                        .and_then(Value::as_object)
                        .map(|o| {
                            o.iter()
                                .filter_map(|(k, v)| Some((k.clone(), str_of(v)?)))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(SettingDef {
                        name: name.to_ascii_lowercase(),
                        kind,
                        label: get_str(s, "label"),
                        default: get(s, "default").and_then(str_of),
                        options,
                        required: false,
                    })
                })
                .collect(),
            _ => Vec::new(),
        };

        let login = get(src, "login").and_then(|l| {
            let method = get_str(l, "method").unwrap_or_else(|| "post".into());
            match method.as_str() {
                "post" | "form" => Some(Login {
                    method: LoginMethod::Form,
                    path: get_str(l, "path").map(|p| self.template(&p, "login.path")),
                    inputs: get(l, "inputs")
                        .and_then(Value::as_object)
                        .map(|o| {
                            o.iter()
                                .filter_map(|(k, v)| Some((k.clone(), str_of(v)?)))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(k, v)| {
                            let context = format!("login.inputs.{k}");
                            (k, self.template(&v, &context))
                        })
                        .collect(),
                    test: get(l, "test").and_then(|t| {
                        Some(LoginTest {
                            path: get_str(t, "path")?,
                            selector: get_str(t, "selector")?,
                        })
                    }),
                }),
                "cookie" => Some(Login {
                    method: LoginMethod::Cookie,
                    path: None,
                    inputs: BTreeMap::new(),
                    test: None,
                }),
                other => {
                    self.warn(format!(
                        "login method `{other}` isn't supported; log in with a cookie instead"
                    ));
                    None
                }
            }
        });

        let search_src = get(src, "search").cloned().unwrap_or(Value::Null);
        let first_path = match get(&search_src, "paths") {
            Some(Value::Array(paths)) => {
                if paths.len() > 1 {
                    self.warn(format!(
                        "the site has {} search paths (e.g. per category); only the first is used",
                        paths.len()
                    ));
                }
                paths.first().cloned()
            }
            _ => get(&search_src, "path").map(|p| {
                Value::Object(serde_json::Map::from_iter([("path".to_owned(), p.clone())]))
            }),
        }
        .unwrap_or(Value::Null);
        let mut path = get_str(&first_path, "path").unwrap_or_default();
        let method = match get_str(&first_path, "method").as_deref() {
            Some("post") => HttpMethod::Post,
            _ => HttpMethod::Get,
        };
        let mut params = BTreeMap::new();
        if let Some(inputs) = get(&search_src, "inputs").and_then(Value::as_object) {
            for (k, v) in inputs {
                let Some(v) = str_of(v) else { continue };
                if k == "$raw" {
                    let raw = self.template(&v, "search.inputs.$raw");
                    path = format!("{path}{}{raw}", if path.contains('?') { "&" } else { "?" });
                } else {
                    let context = format!("search.inputs.{k}");
                    params.insert(k.clone(), self.template(&v, &context));
                }
            }
        }
        let path = self.template(&path, "search.path");
        if get(&search_src, "keywordsfilters").is_some() {
            self.warn("`keywordsfilters` (rewriting the search text) isn't supported; searches use the text as typed");
        }

        let response = match get(&search_src, "response")
            .and_then(|r| get_str(r, "type"))
            .as_deref()
        {
            Some("json") => {
                self.warn("JSON responses use JSONPath in Hashlark; convert each field's `selector` to a `path`");
                ResponseKind::Json
            }
            Some("xml") => ResponseKind::Xml,
            _ => ResponseKind::Html,
        };
        let rows_src = get(&search_src, "rows").cloned().unwrap_or(Value::Null);
        let rows = get_str(&rows_src, "selector").unwrap_or_else(|| "tr".into());
        for unsupported in ["after", "dateheaders", "remove", "filters"] {
            if get(&rows_src, unsupported).is_some() {
                self.warn(format!("rows.{unsupported} isn't supported"));
            }
        }

        let mut fields = BTreeMap::new();
        if let Some(src_fields) = get(&search_src, "fields").and_then(Value::as_object) {
            for (name, spec) in src_fields {
                let target = match name.as_str() {
                    "title" => FieldName::Title,
                    "details" | "comments" => FieldName::Details,
                    "download" => FieldName::Download,
                    "magnet" => FieldName::Magnet,
                    "infohash" => FieldName::InfoHash,
                    "size" => FieldName::Size,
                    "seeders" => FieldName::Seeders,
                    "leechers" => FieldName::Leechers,
                    "date" => FieldName::Date,
                    "category" => FieldName::Category,
                    n if n.starts_with('_')
                        || [
                            "downloadvolumefactor",
                            "uploadvolumefactor",
                            "grabs",
                            "files",
                            "imdb",
                            "imdbid",
                            "tmdbid",
                            "tvdbid",
                            "description",
                            "poster",
                            "minimumratio",
                            "minimumseedtime",
                            "genre",
                            "year",
                            "category|noappend",
                        ]
                        .contains(&n) =>
                    {
                        continue;
                    }
                    other => {
                        self.warn(format!("field `{other}` has no equivalent and was dropped"));
                        continue;
                    }
                };
                if fields.contains_key(&target) {
                    continue;
                }
                let spec = self.field(spec, &format!("search.fields.{name}"));
                fields.insert(target, spec);
            }
        }

        let download = get(src, "download").and_then(|d| {
            let first = match get(d, "selectors") {
                Some(Value::Array(s)) => s.first().cloned(),
                _ => None,
            }?;
            if get(d, "before").is_some() || get(d, "infohash").is_some() {
                self.warn("download.before / download.infohash aren't supported");
            }
            Some(Download {
                selector: get_str(&first, "selector")?,
                attr: get_str(&first, "attribute").unwrap_or_else(|| "href".into()),
                filters: self.filters(get(&first, "filters"), "download"),
            })
        });

        let uses_imdb =
            path.contains("query.imdb") || params.values().any(|v| v.contains("query.imdb"));
        Definition {
            schema: 1,
            id,
            name: get_str(src, "name").unwrap_or_else(|| raw_id.clone()),
            description: get_str(src, "description").unwrap_or_default(),
            language: get_str(src, "language")
                .map(|l| l.split('-').next().unwrap_or("en").to_owned())
                .unwrap_or_else(|| "en".into()),
            site_type,
            version: 1,
            links,
            caps: Caps {
                categories,
                modes: if uses_imdb {
                    vec![Mode::Search, Mode::Imdb]
                } else {
                    vec![Mode::Search]
                },
                paging: None,
            },
            settings,
            headers: BTreeMap::new(),
            login,
            search: Search {
                method,
                path,
                params,
                headers: BTreeMap::new(),
                response,
                rows,
                fields,
                static_feed: None,
                client_filter: false,
                error_selector: None,
                max_rows: None,
            },
            download,
            test: Some(TestSpec {
                query: "ubuntu".into(),
            }),
        }
    }
}

/// Converts a Cardigann (Jackett) YAML definition.
pub fn convert(cardigann_yaml: &str) -> Result<Conversion, DefinitionError> {
    if cardigann_yaml.len() > 512 * 1024 {
        return Err(DefinitionError::one("the file is larger than 512 KiB"));
    }
    let src: Value = serde_saphyr::from_str(cardigann_yaml)
        .map_err(|e| DefinitionError::one(format!("invalid YAML: {e}")))?;
    if get(&src, "search").is_none() || get(&src, "links").is_none() {
        return Err(DefinitionError::one(
            "this doesn't look like a Jackett/Cardigann definition (no `search` or `links`)",
        ));
    }
    let mut converter = Converter {
        warnings: Vec::new(),
    };
    let def = converter.convert(&src);
    let body = serde_saphyr::to_string(&def)
        .map_err(|e| DefinitionError::one(format!("could not write YAML: {e}")))?;
    let yaml = format!(
        "# Converted from a Jackett (Cardigann) definition by Hashlark.\n# Review the conversion notes before relying on it.\n{body}"
    );
    let (valid, errors) = match compile(def) {
        Ok(_) => (true, Vec::new()),
        Err(e) => (false, e.errors),
    };
    Ok(Conversion {
        yaml,
        warnings: converter.warnings,
        valid,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::load;

    /// A Cardigann definition in the shape Jackett uses (for a made-up site).
    const JACKETT: &str = r#"
---
id: ExampleTracker
name: Example Tracker
description: "An example public tracker"
language: en-US
type: public
encoding: UTF-8
links:
  - https://example-tracker.test/
caps:
  categorymappings:
    - {id: 1, cat: Movies, desc: "Movies"}
    - {id: 2, cat: Movies/HD, desc: "Movies HD"}
    - {id: 5, cat: TV/Anime, desc: "Anime"}
    - {id: 9, cat: PC/0day, desc: "Apps"}
  modes:
    search: [q]
    movie-search: [q, imdbid]
settings:
  - {name: sort, type: select, label: Sort, default: seeders, options: {seeders: Seeders, time: Time}}
  - {name: info_note, type: info, label: About, default: "Just a note"}
search:
  paths:
    - path: "{{ if .Keywords }}search/{{ .Keywords }}/1/{{ else }}latest/{{ end }}"
  inputs:
    sort: "{{ .Config.sort }}"
    cats: "{{ join .Categories \",\" }}"
  keywordsfilters:
    - name: re_replace
      args: ["\\s+", "+"]
  rows:
    selector: "table.list > tbody > tr"
  fields:
    category:
      selector: a[href^="/cat/"]
      attribute: href
      filters:
        - name: regexp
          args: "/cat/(\\d+)"
    title:
      selector: td.name a
    details:
      selector: td.name a
      attribute: href
    download:
      selector: a[href^="magnet:"]
      attribute: href
    size:
      selector: td.size
      remove: span
    seeders:
      selector: td.seeds
    leechers:
      selector: td.leech
    date:
      selector: td.date
      filters:
        - name: timeago
    downloadvolumefactor:
      text: 0
    uploadvolumefactor:
      text: 1
"#;

    #[test]
    fn converts_a_typical_public_definition() {
        let result = convert(JACKETT).unwrap();
        assert!(result.valid, "errors: {:?}\n{}", result.errors, result.yaml);
        let def = load(&result.yaml).unwrap().spec;
        assert_eq!(def.id, "exampletracker");
        assert_eq!(def.language, "en");
        assert_eq!(def.caps.categories.get(&Category::Movies).unwrap(), "1");
        assert_eq!(def.caps.categories.get(&Category::Anime).unwrap(), "5");
        assert_eq!(def.caps.categories.get(&Category::Software).unwrap(), "9");
        assert_eq!(
            def.search.path,
            "{% if query.text %}search/{{ query.text | urlencode }}/1/{% else %}latest/{% endif %}"
        );
        assert_eq!(
            def.search.params["cats"],
            "{{ query.site_categories | join(',') }}"
        );
        assert_eq!(def.search.params["sort"], "{{ cfg.sort }}");
        assert_eq!(def.settings.len(), 1, "info settings are dropped");
        assert!(def.search.fields.contains_key(&FieldName::Download));

        let notes = result.warnings.join("\n");
        for expected in [
            "id `ExampleTracker`",
            "Movies/HD",
            "keywordsfilters",
            "`remove`",
        ] {
            assert!(
                notes.contains(expected),
                "missing `{expected}` in:\n{notes}"
            );
        }
    }

    #[test]
    fn rejects_non_cardigann_input() {
        assert!(convert("schema: 1\nid: x").is_err());
        assert!(convert(": : :").is_err());
    }

    #[test]
    fn converts_templates() {
        let mut c = Converter { warnings: vec![] };
        assert_eq!(
            c.template("q={{ .Keywords }}&k={{ .Config.apikey }}", "t"),
            "q={{ query.text }}&k={{ cfg.apikey }}"
        );
        assert_eq!(
            c.template("{{ .Query.IMDBIDShort }}", "t"),
            "{{ query.imdb | replace('tt', '') }}"
        );
        assert!(c.warnings.is_empty());
        c.template("{{ .Today.Year }}", "t");
        assert_eq!(c.warnings.len(), 1);
    }
}
