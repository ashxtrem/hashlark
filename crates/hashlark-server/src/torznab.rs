// SPDX-License-Identifier: GPL-3.0-or-later

//! A Torznab endpoint, so Sonarr, Radarr, Prowlarr and similar apps can use
//! Hashlark as an indexer: `http://<host>/torznab/api?apikey=<token>`.

use std::fmt::Write as _;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use hashlark_core::provider::newznab;
use hashlark_core::{Category, DownloadTarget, MergedResult, SearchQuery};
use serde::Deserialize;
use time::format_description::well_known::Rfc2822;

use crate::AppState;

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;

#[derive(Debug, Default, Deserialize)]
pub struct TorznabParams {
    t: Option<String>,
    q: Option<String>,
    cat: Option<String>,
    apikey: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    imdbid: Option<String>,
    season: Option<String>,
    ep: Option<String>,
}

fn xml_response(status: StatusCode, body: String) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
        body,
    )
        .into_response()
}

fn error(status: StatusCode, code: u32, description: &str) -> Response {
    xml_response(
        status,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><error code="{code}" description="{}"/>"#,
            escape(description)
        ),
    )
}

async fn authorized(
    state: &AppState,
    headers: &HeaderMap,
    apikey: Option<&str>,
    ip: std::net::IpAddr,
) -> Result<(), Box<Response>> {
    let key = apikey.or_else(|| crate::auth::bearer(headers));
    state
        .check_credentials(key, ip)
        .await
        .map_err(|status| Box::new(error(status, 100, "Incorrect API key")))
}

/// `GET /torznab/api`
pub async fn api(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Query(params): Query<TorznabParams>,
) -> Response {
    let t = params.t.as_deref().unwrap_or("caps");
    if t == "caps" {
        return xml_response(StatusCode::OK, caps());
    }
    if let Err(response) = authorized(&state, &headers, params.apikey.as_deref(), addr.ip()).await {
        return *response;
    }
    if !matches!(t, "search" | "tvsearch" | "movie" | "music" | "book") {
        return error(StatusCode::BAD_REQUEST, 202, "No such function");
    }

    let mut text = params.q.clone().unwrap_or_default().trim().to_owned();
    if t == "tvsearch"
        && let Some(season) = params.season.as_deref().and_then(|s| s.parse::<u32>().ok())
    {
        match params.ep.as_deref().and_then(|e| e.parse::<u32>().ok()) {
            Some(ep) => write!(text, " S{season:02}E{ep:02}").expect("string write"),
            None => write!(text, " S{season:02}").expect("string write"),
        }
    }
    let imdb = params
        .imdbid
        .as_deref()
        .map(str::trim)
        .filter(|i| !i.is_empty())
        .map(|i| {
            if i.starts_with("tt") {
                i.to_owned()
            } else {
                format!("tt{i}")
            }
        });
    let mut categories: Vec<Category> = params
        .cat
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .filter_map(|c| c.trim().parse::<u32>().ok())
        .filter_map(newznab::category_of)
        .collect();
    categories.sort_by_key(|c| c.as_str());
    categories.dedup();

    let base = public_base(&headers);
    let apikey = params.apikey.clone().unwrap_or_default();
    let query = SearchQuery {
        categories,
        imdb_id: imdb,
        ..SearchQuery::text(text.trim())
    };
    // An empty search (e.g. an RSS sync) gets an empty feed, not an error.
    if query.is_empty() {
        return xml_response(StatusCode::OK, feed(&[], &base, &apikey));
    }
    match state.engine.search_all(query).await {
        Ok(outcome) => {
            let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
            let offset = params.offset.unwrap_or(0);
            let page: Vec<MergedResult> = outcome
                .results
                .into_iter()
                .skip(offset)
                .take(limit)
                .collect();
            xml_response(StatusCode::OK, feed(&page, &base, &apikey))
        }
        Err(e) => error(StatusCode::BAD_REQUEST, 201, &e.to_string()),
    }
}

/// `GET /torznab/download/{id}`: resolves a result that needs a details
/// page and redirects to its magnet or `.torrent`.
pub async fn download(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(params): Query<TorznabParams>,
) -> Response {
    if let Err(response) = authorized(&state, &headers, params.apikey.as_deref(), addr.ip()).await {
        return *response;
    }
    match state.engine.resolve(&id, None).await {
        Ok(DownloadTarget::Magnet(m)) => Redirect::to(&m).into_response(),
        Ok(DownloadTarget::TorrentFile(u)) => Redirect::to(u.as_str()).into_response(),
        Err(e) => error(StatusCode::NOT_FOUND, 300, &e.to_string()),
    }
}

/// Scheme and host the client used, for links back to this server.
fn public_base(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    format!("{scheme}://{host}")
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

fn caps() -> String {
    let categories: String = [
        (2000, "Movies"),
        (5000, "TV"),
        (3000, "Audio"),
        (4000, "PC"),
        (1000, "Console"),
        (7000, "Books"),
        (8000, "Other"),
    ]
    .iter()
    .map(|(id, name)| {
        if *id == 5000 {
            format!(r#"<category id="{id}" name="{name}"><subcat id="5070" name="TV/Anime"/></category>"#)
        } else {
            format!(r#"<category id="{id}" name="{name}"/>"#)
        }
    })
    .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<caps>
  <server version="{version}" title="Hashlark"/>
  <limits default="{DEFAULT_LIMIT}" max="{MAX_LIMIT}"/>
  <searching>
    <search available="yes" supportedParams="q"/>
    <tv-search available="yes" supportedParams="q,season,ep"/>
    <movie-search available="yes" supportedParams="q,imdbid"/>
    <music-search available="yes" supportedParams="q"/>
    <audio-search available="yes" supportedParams="q"/>
    <book-search available="yes" supportedParams="q"/>
  </searching>
  <categories>{categories}</categories>
</caps>"#,
        version = hashlark_core::VERSION
    )
}

fn feed(results: &[MergedResult], base: &str, apikey: &str) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom" xmlns:torznab="http://torznab.com/schemas/2015/feed">
<channel><title>Hashlark</title><description>Hashlark search results</description>
"#,
    );
    for r in results {
        let p = &r.primary;
        let magnet = p.magnet.clone().or_else(|| {
            p.info_hash.map(|h| {
                hashlark_core::magnet::build_magnet(
                    &h,
                    &p.title,
                    hashlark_core::magnet::DEFAULT_TRACKERS,
                )
            })
        });
        let link = if p.needs_resolve {
            format!(
                "{base}/torznab/download/{}?apikey={}",
                r.id,
                url::form_urlencoded::byte_serialize(apikey.as_bytes()).collect::<String>()
            )
        } else if let Some(m) = &magnet {
            m.clone()
        } else if let Some(u) = &p.torrent_url {
            u.to_string()
        } else {
            continue;
        };
        let size = p.size_bytes.unwrap_or(0);
        let category = p.category.map_or(8000, newznab::id_of);
        let published = p
            .published
            .unwrap_or_else(time::OffsetDateTime::now_utc)
            .format(&Rfc2822)
            .unwrap_or_default();
        let _ = write!(
            out,
            "<item><title>{title}</title><guid isPermaLink=\"false\">{id}</guid><link>{link}</link>",
            title = escape(&p.title),
            id = escape(&r.id),
            link = escape(&link),
        );
        if let Some(details) = &p.details_url {
            let _ = write!(out, "<comments>{}</comments>", escape(details.as_str()));
        }
        let _ = write!(
            out,
            "<pubDate>{published}</pubDate><size>{size}</size><category>{category}</category>\
             <enclosure url=\"{link}\" length=\"{size}\" type=\"application/x-bittorrent\"/>\
             <torznab:attr name=\"category\" value=\"{category}\"/>",
            link = escape(&link),
        );
        if let Some(s) = r.seeders {
            let _ = write!(out, "<torznab:attr name=\"seeders\" value=\"{s}\"/>");
            let peers = s + r.leechers.unwrap_or(0);
            let _ = write!(out, "<torznab:attr name=\"peers\" value=\"{peers}\"/>");
        }
        if let Some(h) = &p.info_hash {
            let _ = write!(out, "<torznab:attr name=\"infohash\" value=\"{h}\"/>");
        }
        if let Some(m) = &magnet {
            let _ = write!(
                out,
                "<torznab:attr name=\"magneturl\" value=\"{}\"/>",
                escape(m)
            );
        }
        out.push_str("</item>\n");
    }
    out.push_str("</channel></rss>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_xml() {
        assert_eq!(escape(r#"a&b<c>"d'"#), "a&amp;b&lt;c&gt;&quot;d&apos;");
        assert_eq!(escape("bell\u{7}"), "bell");
    }

    #[test]
    fn caps_is_valid_xml() {
        let xml = caps();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(doc.root_element().tag_name().name(), "caps");
    }
}
