// SPDX-License-Identifier: GPL-3.0-or-later

//! Validates a [`Definition`] and prepares everything needed at search time
//! (selectors, paths, filters, templates), so errors surface on import, not
//! mid-search.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use scraper::Selector;
use serde::Serialize;
use serde_json_path::JsonPath;
use url::Url;

use super::filters::{self, Filter};
use super::spec::{Definition, FieldName, FieldSpec, LoginMethod, Mode, ResponseKind, SettingKind};
use super::xmlpath::XmlPath;
use crate::model::Category;

/// Why a definition was rejected. Holds every problem found, not just the
/// first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DefinitionError {
    pub errors: Vec<String>,
}

impl DefinitionError {
    pub fn one(message: impl Into<String>) -> Self {
        Self {
            errors: vec![message.into()],
        }
    }
}

impl fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.errors.join("; "))
    }
}

impl std::error::Error for DefinitionError {}

/// Where rows come from.
#[derive(Debug)]
pub enum Rows {
    Css(Selector),
    Json(JsonPath),
    Xml(XmlPath),
}

/// Where one field comes from.
#[derive(Debug)]
pub enum Source {
    Css(Selector),
    Json(JsonPath),
    Xml(XmlPath),
    /// A template (name of the template in the environment).
    Template(String),
    /// The row element's own text (or attribute).
    RowItself,
}

#[derive(Debug)]
pub struct CompiledField {
    pub name: FieldName,
    pub source: Source,
    pub attr: Option<String>,
    pub filters: Vec<Filter>,
    pub optional: bool,
}

#[derive(Debug)]
pub struct CompiledDownload {
    pub selector: Selector,
    pub attr: String,
    pub filters: Vec<Filter>,
}

/// A validated definition, ready to run.
#[derive(Debug)]
pub struct CompiledDefinition {
    pub spec: Definition,
    pub links: Vec<Url>,
    pub rows: Rows,
    pub fields: Vec<CompiledField>,
    pub error_selector: Option<Selector>,
    pub download: Option<CompiledDownload>,
    pub login_test_selector: Option<Selector>,
    pub templates: minijinja::Environment<'static>,
    /// Site category id → Hashlark category.
    pub category_by_site_id: HashMap<String, Category>,
}

/// Parses YAML into a definition, with limits suitable for untrusted input.
pub fn parse_yaml(yaml: &str) -> Result<Definition, DefinitionError> {
    if yaml.len() > 256 * 1024 {
        return Err(DefinitionError::one("definition is larger than 256 KiB"));
    }
    serde_saphyr::from_str(yaml).map_err(|e| DefinitionError::one(format!("invalid YAML: {e}")))
}

/// Parses and compiles in one step.
pub fn load(yaml: &str) -> Result<CompiledDefinition, DefinitionError> {
    compile(parse_yaml(yaml)?)
}

fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (2..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn css(selector: &str, what: &str, errors: &mut Vec<String>) -> Option<Selector> {
    Selector::parse(selector)
        .map_err(|e| {
            // The parser's own messages are internal jargon; name the selector instead.
            tracing::debug!(error = %e, selector, "CSS selector rejected");
            errors.push(format!("{what}: invalid CSS selector `{selector}`"));
        })
        .ok()
}

/// Checks and compiles a definition.
pub fn compile(spec: Definition) -> Result<CompiledDefinition, DefinitionError> {
    let mut errors = Vec::new();

    if spec.schema != 1 {
        errors.push(format!(
            "unsupported schema version {} (expected 1)",
            spec.schema
        ));
    }
    if !is_valid_id(&spec.id) {
        errors.push(format!(
            "id `{}` must be 2–64 characters: lowercase letters, digits and dashes, starting with a letter or digit",
            spec.id
        ));
    }
    if spec.name.trim().is_empty() {
        errors.push("name must not be empty".into());
    }

    let mut links = Vec::new();
    if spec.links.is_empty() {
        errors.push("links must list at least one URL".into());
    }
    for link in &spec.links {
        match Url::parse(link) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => {
                // Ensure joins keep the whole base path.
                let mut url = url;
                if !url.path().ends_with('/') {
                    url.set_path(&format!("{}/", url.path()));
                }
                links.push(url);
            }
            _ => errors.push(format!("link `{link}` is not an http(s) URL")),
        }
    }
    if spec.caps.modes.is_empty() {
        errors.push("caps.modes must not be empty".into());
    }

    let mut setting_names = HashSet::new();
    for setting in &spec.settings {
        let valid = !setting.name.is_empty()
            && setting
                .name
                .starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            && setting
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !valid {
            errors.push(format!(
                "setting name `{}` must use lowercase letters, digits and underscores",
                setting.name
            ));
        }
        if !setting_names.insert(setting.name.as_str()) {
            errors.push(format!("setting `{}` is listed twice", setting.name));
        }
        if setting.kind == SettingKind::Select && setting.options.is_empty() {
            errors.push(format!("select setting `{}` needs options", setting.name));
        }
    }

    // Templates.
    let mut templates = minijinja::Environment::new();
    templates.set_fuel(Some(100_000));
    templates.set_recursion_limit(32);
    let mut template_sources: BTreeMap<String, String> = BTreeMap::new();
    let mut add_template = |name: String, source: &str| {
        template_sources.insert(name, source.to_owned());
    };
    add_template("search.path".into(), &spec.search.path);
    for (k, v) in &spec.search.params {
        add_template(format!("search.params.{k}"), v);
    }
    for (k, v) in spec.headers.iter().chain(&spec.search.headers) {
        add_template(format!("headers.{k}"), v);
    }

    if let Some(login) = &spec.login {
        match login.method {
            LoginMethod::Form => {
                match &login.path {
                    Some(path) => add_template("login.path".into(), path),
                    None => errors.push("login.path is required for form login".into()),
                }
                for (k, v) in &login.inputs {
                    add_template(format!("login.inputs.{k}"), v);
                }
            }
            LoginMethod::Cookie => {
                if !spec.settings.iter().any(|s| s.name == "cookie") {
                    errors.push("cookie login needs a setting named `cookie`".into());
                }
            }
        }
    }
    let login_test_selector = spec
        .login
        .as_ref()
        .and_then(|l| l.test.as_ref())
        .and_then(|t| {
            add_template("login.test.path".into(), &t.path);
            css(&t.selector, "login.test.selector", &mut errors)
        });

    // Rows and fields.
    let search = &spec.search;
    let rows = match search.response {
        ResponseKind::Html => css(&search.rows, "search.rows", &mut errors).map(Rows::Css),
        ResponseKind::Json => JsonPath::parse(&search.rows)
            .map(Rows::Json)
            .map_err(|e| {
                errors.push(format!(
                    "search.rows: invalid JSONPath `{}`: {e}",
                    search.rows
                ))
            })
            .ok(),
        ResponseKind::Xml => XmlPath::parse(&search.rows)
            .map(Rows::Xml)
            .map_err(|e| errors.push(format!("search.rows: {e}")))
            .ok(),
    };

    if !search.fields.contains_key(&FieldName::Title) {
        errors.push("search.fields must include `title`".into());
    }
    let has_link = [FieldName::Magnet, FieldName::Download, FieldName::InfoHash]
        .iter()
        .any(|f| search.fields.contains_key(f));
    if !has_link && !(spec.download.is_some() && search.fields.contains_key(&FieldName::Details)) {
        errors.push(
            "results need a way to download: a `magnet`, `download` or `info_hash` field, or a `details` field plus a `download` section"
                .into(),
        );
    }

    let mut fields = Vec::new();
    for name in FieldName::ORDER {
        let Some(field) = search.fields.get(&name) else {
            continue;
        };
        let what = format!("search.fields.{}", name.as_str());
        if let Some(source) = compile_source(field, search.response, &what, &mut errors) {
            if let Source::Template(t) = &source {
                add_template(t.clone(), field.text.as_deref().unwrap_or_default());
            }
            match filters::compile_all(&field.filters) {
                Ok(filters) => fields.push(CompiledField {
                    name,
                    source,
                    attr: field.attr.clone(),
                    filters,
                    optional: field.optional,
                }),
                Err(e) => errors.push(format!("{what}: {e}")),
            }
        }
    }

    let error_selector = search
        .error_selector
        .as_deref()
        .and_then(|s| css(s, "search.error_selector", &mut errors));
    if search.error_selector.is_some() && search.response != ResponseKind::Html {
        errors.push("search.error_selector only works with html responses".into());
    }

    let download = spec.download.as_ref().and_then(|d| {
        let selector = css(&d.selector, "download.selector", &mut errors)?;
        match filters::compile_all(&d.filters) {
            Ok(filters) => Some(CompiledDownload {
                selector,
                attr: d.attr.clone(),
                filters,
            }),
            Err(e) => {
                errors.push(format!("download: {e}"));
                None
            }
        }
    });

    for (name, source) in template_sources {
        if let Err(e) = templates.add_template_owned(name.clone(), source) {
            errors.push(format!("{name}: invalid template: {e}"));
        }
    }

    if spec.caps.modes.contains(&Mode::Imdb)
        && !spec.search.path.contains("query.imdb")
        && !spec
            .search
            .params
            .values()
            .any(|v| v.contains("query.imdb"))
    {
        errors.push("caps.modes includes `imdb` but no template uses `query.imdb`".into());
    }

    let category_by_site_id = spec
        .caps
        .categories
        .iter()
        .map(|(cat, id)| (id.clone(), *cat))
        .collect();

    match (errors.is_empty(), rows) {
        (true, Some(rows)) => Ok(CompiledDefinition {
            spec,
            links,
            rows,
            fields,
            error_selector,
            download,
            login_test_selector,
            templates,
            category_by_site_id,
        }),
        _ => Err(DefinitionError { errors }),
    }
}

fn compile_source(
    field: &FieldSpec,
    response: ResponseKind,
    what: &str,
    errors: &mut Vec<String>,
) -> Option<Source> {
    let set = [
        field.selector.is_some(),
        field.path.is_some(),
        field.text.is_some(),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    if set > 1 {
        errors.push(format!("{what}: use only one of selector, path or text"));
        return None;
    }
    if let Some(_text) = &field.text {
        return Some(Source::Template(format!("{what}.text")));
    }
    match response {
        ResponseKind::Html => {
            if field.path.is_some() {
                errors.push(format!("{what}: html responses use `selector`, not `path`"));
                return None;
            }
            match &field.selector {
                Some(s) => css(s, what, errors).map(Source::Css),
                None => Some(Source::RowItself),
            }
        }
        ResponseKind::Json | ResponseKind::Xml => {
            if field.selector.is_some() {
                errors.push(format!(
                    "{what}: {response:?} responses use `path`, not `selector`"
                ));
                return None;
            }
            if field.attr.is_some() {
                errors.push(format!(
                    "{what}: `attr` is for html; use `/@name` at the end of an XML path"
                ));
                return None;
            }
            let Some(path) = &field.path else {
                errors.push(format!("{what}: needs a `path` (or `text`)"));
                return None;
            };
            if response == ResponseKind::Json {
                JsonPath::parse(path)
                    .map(Source::Json)
                    .map_err(|e| errors.push(format!("{what}: invalid JSONPath `{path}`: {e}")))
                    .ok()
            } else {
                XmlPath::parse(path)
                    .map(Source::Xml)
                    .map_err(|e| errors.push(format!("{what}: {e}")))
                    .ok()
            }
        }
    }
}
