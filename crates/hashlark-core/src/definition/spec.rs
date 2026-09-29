// SPDX-License-Identifier: GPL-3.0-or-later

//! The YAML definition format (schema version 1). See
//! `docs/definition-format.md` for the user-facing reference.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::Category;

/// A provider definition: describes a site as data, never code.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Definition {
    /// Format version. Must be `1`.
    pub schema: u32,
    /// Unique id: lowercase letters, digits and dashes, e.g. `linuxtracker`.
    pub id: String,
    /// Display name.
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Main language of the site, e.g. `en`.
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default, rename = "type")]
    pub site_type: SiteType,
    /// Increase on every change, so repositories can deliver updates.
    #[serde(default = "one")]
    pub version: u32,
    /// Base URLs, tried in order. `.onion` links are used only with Tor.
    pub links: Vec<String>,
    #[serde(default)]
    pub caps: Caps,
    /// Values the user fills in (credentials, API keys, options).
    #[serde(default)]
    pub settings: Vec<SettingDef>,
    /// Headers sent with every request. Values are templates.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub login: Option<Login>,
    pub search: Search,
    /// How to find the download link on a result's details page, for sites
    /// whose result rows don't carry it.
    #[serde(default)]
    pub download: Option<Download>,
    /// Query used by "Test" and health checks.
    #[serde(default)]
    pub test: Option<TestSpec>,
}

fn default_language() -> String {
    "en".into()
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SiteType {
    #[default]
    Public,
    SemiPrivate,
    Private,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Caps {
    /// Hashlark category → the site's own category id. Categories left out
    /// aren't searched on this site. Empty means the site has any content.
    #[serde(default)]
    pub categories: BTreeMap<Category, String>,
    #[serde(default = "default_modes")]
    pub modes: Vec<Mode>,
    #[serde(default)]
    pub paging: Option<Paging>,
}

fn default_modes() -> Vec<Mode> {
    vec![Mode::Search]
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            categories: BTreeMap::new(),
            modes: default_modes(),
            paging: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Free-text search.
    Search,
    /// Search by IMDb id (`{{ query.imdb }}`).
    Imdb,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Paging {
    /// The site's number for the first page (usually 0 or 1). Available to
    /// templates as `query.page`.
    #[serde(default = "one")]
    pub start: u32,
    /// Results per page, for sites paged by offset (`query.offset`).
    #[serde(default)]
    pub size: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SettingDef {
    /// Name used in templates as `cfg.<name>`.
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: SettingKind,
    /// Label shown to the user. Defaults to the name.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
    /// For `select`: value → label.
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    #[default]
    Text,
    /// Stored in the OS keychain, never shown again.
    Password,
    Checkbox,
    Select,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Login {
    pub method: LoginMethod,
    /// For `form`: where the login form posts to.
    #[serde(default)]
    pub path: Option<String>,
    /// For `form`: form fields. Values are templates, e.g. `"{{ cfg.username }}"`.
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    /// Checks that login worked.
    #[serde(default)]
    pub test: Option<LoginTest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum LoginMethod {
    /// Post a login form once; the session cookie is kept.
    Form,
    /// Send the `cookie` setting as the `Cookie` header on every request.
    Cookie,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct LoginTest {
    /// Page to load after logging in.
    pub path: String,
    /// CSS selector that exists only when logged in (e.g. a logout link).
    pub selector: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ResponseKind {
    /// Rows and fields use CSS selectors.
    Html,
    /// Rows and fields use JSONPath (`$` is the row inside fields).
    Json,
    /// RSS/XML. Rows and fields use simple paths like `channel/item` or
    /// `enclosure/@url`.
    Xml,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Search {
    #[serde(default)]
    pub method: HttpMethod,
    /// Path (or full URL) of the search, relative to the chosen link.
    /// Template, e.g. `/search/{{ query.text | urlencode }}/{{ query.page }}/`.
    pub path: String,
    /// Query parameters (GET) or form fields (POST). Values are templates.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub response: ResponseKind,
    /// Selects one element per result.
    pub rows: String,
    /// How to read each result field from a row.
    pub fields: BTreeMap<FieldName, FieldSpec>,
    /// The page doesn't depend on the query (e.g. a full RSS feed): fetch it
    /// once, cache it, and filter results locally.
    #[serde(default)]
    pub static_feed: Option<StaticFeed>,
    /// Keep only rows whose title contains every query word. Implied by
    /// `static_feed`.
    #[serde(default)]
    pub client_filter: bool,
    /// HTML only: if this selector matches, the site reported an error.
    #[serde(default)]
    pub error_selector: Option<String>,
    /// Stop after this many rows.
    #[serde(default)]
    pub max_rows: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct StaticFeed {
    /// How long a fetched feed is reused.
    #[serde(default = "six")]
    pub ttl_hours: u32,
}

fn six() -> u32 {
    6
}

/// The result fields a definition can fill in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FieldName {
    Title,
    /// Link to the result's page on the site.
    Details,
    /// Link to the `.torrent` file (a magnet link here also works).
    Download,
    Magnet,
    InfoHash,
    /// Size in bytes, or text like `1.4 GiB`.
    Size,
    Seeders,
    Leechers,
    /// Publication date: RFC 3339, RFC 2822, Unix seconds, `YYYY-MM-DD`, or
    /// relative text like `3 days ago`.
    Date,
    /// The site's category id (mapped back through `caps.categories`) or a
    /// Hashlark category name.
    Category,
}

impl FieldName {
    /// Extraction order: later fields' `text` templates can use earlier ones.
    pub const ORDER: [FieldName; 10] = [
        Self::Title,
        Self::Details,
        Self::Download,
        Self::Magnet,
        Self::InfoHash,
        Self::Size,
        Self::Seeders,
        Self::Leechers,
        Self::Date,
        Self::Category,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Details => "details",
            Self::Download => "download",
            Self::Magnet => "magnet",
            Self::InfoHash => "info_hash",
            Self::Size => "size",
            Self::Seeders => "seeders",
            Self::Leechers => "leechers",
            Self::Date => "date",
            Self::Category => "category",
        }
    }
}

/// Where a field's value comes from. Use one of `selector` (HTML), `path`
/// (JSON/XML) or `text` (a template); with none, the row's own text is used.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    /// CSS selector inside the row (HTML).
    #[serde(default)]
    pub selector: Option<String>,
    /// JSONPath or XML path inside the row.
    #[serde(default)]
    pub path: Option<String>,
    /// HTML: read this attribute instead of the text.
    #[serde(default)]
    pub attr: Option<String>,
    /// A template instead of reading the page. Can use `row.<field>` for
    /// fields listed earlier.
    #[serde(default)]
    pub text: Option<String>,
    /// Transformations applied in order.
    #[serde(default)]
    pub filters: Vec<FilterSpec>,
    /// A missing value isn't an error.
    #[serde(default)]
    pub optional: bool,
}

/// A filter: either a bare name (`trim`) or a one-key map with arguments
/// (`regex: "id=(\\d+)"`, `replace: [",", ""]`).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum FilterSpec {
    Name(String),
    WithArgs(BTreeMap<String, FilterArgs>),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum FilterArgs {
    One(String),
    Number(i64),
    Many(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Download {
    /// CSS selector for the link on the details page.
    pub selector: String,
    #[serde(default = "href")]
    pub attr: String,
    #[serde(default)]
    pub filters: Vec<FilterSpec>,
}

fn href() -> String {
    "href".into()
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct TestSpec {
    pub query: String,
}
