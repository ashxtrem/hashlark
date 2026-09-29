// SPDX-License-Identifier: GPL-3.0-or-later

//! Turns a fetched page into results according to a compiled definition.

use std::collections::BTreeMap;

use scraper::{ElementRef, Html};
use serde::Serialize;
use serde_json::Value;
use url::Url;

use super::compile::{CompiledDefinition, CompiledField, Rows, Source};
use super::filters::{self, parse_date_any, parse_size, to_int};
use super::spec::{FieldName, ResponseKind};
use crate::error::ProviderError;
use crate::magnet::infohash_from_magnet;
use crate::model::{Category, InfoHash, ProviderId, SearchQuery, SearchResult};
use crate::ranking;

/// Values available to templates.
#[derive(Debug, Serialize)]
pub struct TemplateContext<'a> {
    pub query: QueryContext,
    pub cfg: &'a BTreeMap<String, String>,
    /// Fields extracted so far for the current row.
    pub row: BTreeMap<&'static str, String>,
    /// The link (base URL) in use.
    pub base: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryContext {
    pub text: String,
    /// The site's page number (paging start + page − 1).
    pub page: u32,
    /// Zero-based result offset (page − 1) × page size.
    pub offset: u32,
    /// Hashlark category names requested.
    pub categories: Vec<String>,
    /// The site's ids for the requested categories.
    pub site_categories: Vec<String>,
    pub imdb: String,
}

impl QueryContext {
    pub fn new(def: &CompiledDefinition, query: &SearchQuery) -> Self {
        let paging = def.spec.caps.paging.as_ref();
        let page0 = query.page.max(1) - 1;
        Self {
            text: query.text.trim().to_owned(),
            page: paging.map_or(1, |p| p.start) + page0,
            offset: page0 * paging.and_then(|p| p.size).unwrap_or(0),
            categories: query
                .categories
                .iter()
                .map(|c| c.as_str().to_owned())
                .collect(),
            site_categories: query
                .categories
                .iter()
                .filter_map(|c| def.spec.caps.categories.get(c).cloned())
                .collect(),
            imdb: query.imdb_id.clone().unwrap_or_default(),
        }
    }
}

pub fn render(
    def: &CompiledDefinition,
    template: &str,
    ctx: &TemplateContext<'_>,
) -> Result<String, ProviderError> {
    def.templates
        .get_template(template)
        .and_then(|t| t.render(ctx))
        .map_err(|e| ProviderError::Definition(format!("{template}: {e}")))
}

type RawRow = BTreeMap<FieldName, String>;

/// Parses a search response. `page_url` resolves relative links.
pub fn parse_page(
    def: &CompiledDefinition,
    provider_id: &ProviderId,
    body: &str,
    page_url: &Url,
    query: &QueryContext,
    cfg: &BTreeMap<String, String>,
) -> Result<Vec<SearchResult>, ProviderError> {
    let max_rows = def.spec.search.max_rows.unwrap_or(1000);
    let base = TemplateContext {
        query: query.clone(),
        cfg,
        row: BTreeMap::new(),
        base: page_url.to_string(),
    };
    let raw_rows = match def.spec.search.response {
        ResponseKind::Html => html_rows(def, body, page_url, &base, max_rows)?,
        ResponseKind::Json => json_rows(def, body, page_url, &base, max_rows)?,
        ResponseKind::Xml => xml_rows(def, body, page_url, &base, max_rows)?,
    };
    check_required(def, &raw_rows)?;

    let mut results: Vec<SearchResult> = raw_rows
        .into_iter()
        .filter_map(|row| finalize(def, provider_id, row, page_url))
        .collect();

    if def.spec.search.client_filter || def.spec.search.static_feed.is_some() {
        let words = ranking::tokens(&query.text);
        results.retain(|r| {
            let title = ranking::tokens(&r.title);
            words.iter().all(|w| title.contains(w))
        });
        results.truncate(200);
    }
    Ok(results)
}

fn field_value(
    def: &CompiledDefinition,
    field: &CompiledField,
    extracted: Option<String>,
    ctx: &TemplateContext<'_>,
    page_url: &Url,
) -> Result<Option<String>, ProviderError> {
    let raw = match &field.source {
        Source::Template(name) => Some(render(def, name, ctx)?),
        _ => extracted,
    };
    Ok(filters::apply(&field.filters, raw, page_url))
}

/// A required field missing from every row means the site changed.
fn check_required(def: &CompiledDefinition, rows: &[RawRow]) -> Result<(), ProviderError> {
    if rows.is_empty() {
        return Ok(());
    }
    for field in def.fields.iter().filter(|f| !f.optional) {
        if rows.iter().all(|row| !row.contains_key(&field.name)) {
            return Err(ProviderError::ParseFailed {
                field: field.name.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

fn html_rows(
    def: &CompiledDefinition,
    body: &str,
    page_url: &Url,
    base: &TemplateContext<'_>,
    max_rows: usize,
) -> Result<Vec<RawRow>, ProviderError> {
    let doc = Html::parse_document(body);
    if let Some(sel) = &def.error_selector
        && let Some(el) = doc.select(sel).next()
    {
        return Err(ProviderError::Definition(format!(
            "the site reported an error: {}",
            element_text(el)
        )));
    }
    let Rows::Css(rows) = &def.rows else {
        unreachable!("compile checks the response kind")
    };
    let mut out = Vec::new();
    for row in doc.select(rows).take(max_rows) {
        let mut ctx_row = BTreeMap::new();
        let mut values = RawRow::new();
        for field in &def.fields {
            let extracted = match &field.source {
                Source::Css(sel) => row.select(sel).next().map(|el| read_element(el, field)),
                Source::RowItself => Some(read_element(row, field)),
                _ => None,
            }
            .flatten();
            let ctx = TemplateContext {
                row: ctx_row.clone(),
                ..clone_ctx(base)
            };
            let value = field_value(def, field, extracted, &ctx, page_url)?;
            if let Some(v) = value {
                ctx_row.insert(field.name.as_str(), v.clone());
                values.insert(field.name, v);
            }
        }
        out.push(values);
    }
    Ok(out)
}

fn clone_ctx<'a>(ctx: &TemplateContext<'a>) -> TemplateContext<'a> {
    TemplateContext {
        query: ctx.query.clone(),
        cfg: ctx.cfg,
        row: BTreeMap::new(),
        base: ctx.base.clone(),
    }
}

fn read_element(el: ElementRef<'_>, field: &CompiledField) -> Option<String> {
    match &field.attr {
        Some(attr) => el.value().attr(attr).map(str::to_owned),
        None => Some(element_text(el)),
    }
}

fn element_text(el: ElementRef<'_>) -> String {
    el.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn json_scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(items) => items.first().and_then(json_scalar),
        Value::Null | Value::Object(_) => None,
    }
}

fn json_rows(
    def: &CompiledDefinition,
    body: &str,
    page_url: &Url,
    base: &TemplateContext<'_>,
    max_rows: usize,
) -> Result<Vec<RawRow>, ProviderError> {
    let doc: Value = serde_json::from_str(body).map_err(|_| ProviderError::ParseFailed {
        field: "body (not JSON)".into(),
    })?;
    let Rows::Json(rows) = &def.rows else {
        unreachable!("compile checks the response kind")
    };
    let mut out = Vec::new();
    for row in rows.query(&doc).all().into_iter().take(max_rows) {
        let mut ctx_row = BTreeMap::new();
        let mut values = RawRow::new();
        for field in &def.fields {
            let extracted = match &field.source {
                Source::Json(path) => path.query(row).all().first().and_then(|v| json_scalar(v)),
                _ => None,
            };
            let ctx = TemplateContext {
                row: ctx_row.clone(),
                ..clone_ctx(base)
            };
            let value = field_value(def, field, extracted, &ctx, page_url)?;
            if let Some(v) = value {
                ctx_row.insert(field.name.as_str(), v.clone());
                values.insert(field.name, v);
            }
        }
        out.push(values);
    }
    Ok(out)
}

fn xml_rows(
    def: &CompiledDefinition,
    body: &str,
    page_url: &Url,
    base: &TemplateContext<'_>,
    max_rows: usize,
) -> Result<Vec<RawRow>, ProviderError> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(body, options).map_err(|_| {
        ProviderError::ParseFailed {
            field: "body (not XML)".into(),
        }
    })?;
    let Rows::Xml(rows) = &def.rows else {
        unreachable!("compile checks the response kind")
    };
    let mut out = Vec::new();
    for row in rows.select_from_document(&doc).into_iter().take(max_rows) {
        let mut ctx_row = BTreeMap::new();
        let mut values = RawRow::new();
        for field in &def.fields {
            let extracted = match &field.source {
                Source::Xml(path) => path.value(row),
                _ => None,
            };
            let ctx = TemplateContext {
                row: ctx_row.clone(),
                ..clone_ctx(base)
            };
            let value = field_value(def, field, extracted, &ctx, page_url)?;
            if let Some(v) = value {
                ctx_row.insert(field.name.as_str(), v.clone());
                values.insert(field.name, v);
            }
        }
        out.push(values);
    }
    Ok(out)
}

/// Converts raw field text into a typed result. Rows without a title are
/// dropped.
fn finalize(
    def: &CompiledDefinition,
    provider_id: &ProviderId,
    mut row: RawRow,
    page_url: &Url,
) -> Option<SearchResult> {
    let title = row.remove(&FieldName::Title)?.trim().to_owned();
    if title.is_empty() {
        return None;
    }
    let join = |s: String| {
        page_url
            .join(s.trim())
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
    };

    let mut magnet = row
        .remove(&FieldName::Magnet)
        .filter(|m| m.trim_start().starts_with("magnet:"));
    let mut torrent_url = None;
    if let Some(download) = row.remove(&FieldName::Download) {
        if download.trim_start().starts_with("magnet:") {
            magnet = magnet.or(Some(download));
        } else {
            torrent_url = join(download);
        }
    }
    let info_hash: Option<InfoHash> = row
        .remove(&FieldName::InfoHash)
        .and_then(|h| h.trim().parse().ok())
        .or_else(|| magnet.as_deref().and_then(infohash_from_magnet));
    let details_url = row.remove(&FieldName::Details).and_then(join);
    let size_bytes = row.remove(&FieldName::Size).and_then(|s| parse_size(&s));
    let seeders = row
        .remove(&FieldName::Seeders)
        .and_then(|s| to_int(&s))
        .and_then(|n| u32::try_from(n).ok());
    let leechers = row
        .remove(&FieldName::Leechers)
        .and_then(|s| to_int(&s))
        .and_then(|n| u32::try_from(n).ok());
    let published = row
        .remove(&FieldName::Date)
        .and_then(|d| parse_date_any(&d));
    let category = row.remove(&FieldName::Category).and_then(|c| {
        let c = c.trim();
        def.category_by_site_id
            .get(c)
            .copied()
            .or_else(|| c.parse::<Category>().ok())
    });

    let has_link = magnet.is_some() || info_hash.is_some() || torrent_url.is_some();
    let needs_resolve = !has_link && def.download.is_some() && details_url.is_some();
    if !has_link && !needs_resolve {
        return None;
    }
    Some(SearchResult {
        title,
        size_bytes,
        seeders,
        leechers,
        info_hash,
        magnet,
        torrent_url,
        details_url,
        published,
        category,
        provider_id: provider_id.clone(),
        needs_resolve,
    })
}
