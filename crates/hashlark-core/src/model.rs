// SPDX-License-Identifier: GPL-3.0-or-later

//! Domain types shared by providers, the aggregator and every front end.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use url::Url;

/// Content category. Providers map their own category ids onto these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Movies,
    Tv,
    Music,
    Books,
    Software,
    Games,
    Anime,
    Other,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Self::Movies,
        Self::Tv,
        Self::Music,
        Self::Books,
        Self::Software,
        Self::Games,
        Self::Anime,
        Self::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movies => "movies",
            Self::Tv => "tv",
            Self::Music => "music",
            Self::Books => "books",
            Self::Software => "software",
            Self::Games => "games",
            Self::Anime => "anime",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

impl FromStr for Category {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_str().eq_ignore_ascii_case(s))
            .ok_or_else(|| format!("unknown category `{s}`"))
    }
}

/// Stable identifier of a provider, e.g. `internet-archive`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.0)
    }
}

impl From<&str> for ProviderId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

/// A BitTorrent v1 infohash (SHA-1, 20 bytes).
///
/// Parses from 40-character hex or 32-character base32 (both appear in
/// magnet links) and always displays and serializes as lowercase hex, so
/// equal torrents compare equal regardless of how a provider wrote them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InfoHash([u8; 20]);

impl InfoHash {
    pub fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.0)
    }
}

impl FromStr for InfoHash {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let decoded = match s.len() {
            40 => data_encoding::HEXLOWER_PERMISSIVE.decode(s.as_bytes()),
            32 => data_encoding::BASE32.decode(s.to_ascii_uppercase().as_bytes()),
            n => {
                return Err(format!(
                    "infohash must be 40 hex or 32 base32 chars, got {n}"
                ));
            }
        }
        .map_err(|e| format!("invalid infohash `{s}`: {e}"))?;
        let bytes: [u8; 20] = decoded
            .try_into()
            .map_err(|_| format!("invalid infohash `{s}`: wrong length"))?;
        Ok(Self(bytes))
    }
}

impl fmt::Display for InfoHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.to_hex())
    }
}

impl fmt::Debug for InfoHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InfoHash({})", self.to_hex())
    }
}

impl Serialize for InfoHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for InfoHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// How the merged result list is ordered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Relevance,
    Title,
    Seeders,
    Peers,
    Size,
    Date,
}

impl FromStr for SortOrder {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "relevance" => Ok(Self::Relevance),
            "title" => Ok(Self::Title),
            "seeders" => Ok(Self::Seeders),
            "peers" => Ok(Self::Peers),
            "size" => Ok(Self::Size),
            "date" => Ok(Self::Date),
            _ => Err(format!("unknown sort order `{s}`")),
        }
    }
}

/// A search request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchQuery {
    pub text: String,
    /// Empty means all categories.
    #[serde(default)]
    pub categories: Vec<Category>,
    /// `None` means every enabled provider.
    #[serde(default)]
    pub providers: Option<Vec<ProviderId>>,
    #[serde(default)]
    pub imdb_id: Option<String>,
    /// 1-based page number.
    #[serde(default = "first_page")]
    pub page: u32,
    #[serde(default)]
    pub sort: SortOrder,
}

fn first_page() -> u32 {
    1
}

impl SearchQuery {
    /// A text query with default options.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            categories: Vec::new(),
            providers: None,
            imdb_id: None,
            page: 1,
            sort: SortOrder::default(),
        }
    }

    /// Whether the query has anything to search for.
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.imdb_id.as_deref().is_none_or(str::is_empty)
    }
}

/// One result as returned by one provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchResult {
    pub title: String,
    pub size_bytes: Option<u64>,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub info_hash: Option<InfoHash>,
    pub magnet: Option<String>,
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub torrent_url: Option<Url>,
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub details_url: Option<Url>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>, format = DateTime))]
    pub published: Option<OffsetDateTime>,
    pub category: Option<Category>,
    pub provider_id: ProviderId,
    /// The download link is only available after [`resolve`] (e.g. it is on
    /// the provider's details page).
    ///
    /// [`resolve`]: crate::provider::SearchProvider::resolve
    #[serde(default)]
    pub needs_resolve: bool,
}

impl SearchResult {
    /// A result with only a title; fill the rest with struct update syntax.
    pub fn new(provider_id: ProviderId, title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            size_bytes: None,
            seeders: None,
            leechers: None,
            info_hash: None,
            magnet: None,
            torrent_url: None,
            details_url: None,
            published: None,
            category: None,
            provider_id,
            needs_resolve: false,
        }
    }
}

/// A result after de-duplication across providers. This is what front ends
/// display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MergedResult {
    /// Stable id: derived from the infohash when known, so the same torrent
    /// keeps its id across searches.
    pub id: String,
    /// Best combined view of the result. Missing fields are filled in from
    /// other sources as they arrive.
    pub primary: SearchResult,
    /// Every provider that returned this torrent, in arrival order.
    pub sources: Vec<ProviderId>,
    /// Highest seeder count reported by any source.
    pub seeders: Option<u32>,
    /// Highest leecher count reported by any source.
    pub leechers: Option<u32>,
    /// Relevance score in `0.0..=1.0`.
    pub score: f32,
}

/// What to hand to the OS to start a download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "url", rename_all = "snake_case")]
pub enum DownloadTarget {
    Magnet(String),
    TorrentFile(Url),
}

/// Which kind of [`DownloadTarget`] a caller would rather have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Magnet,
    TorrentFile,
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX: &str = "1dc689206d6f6ef6021d558d95350ed09004b40c";

    #[test]
    fn infohash_parses_hex_in_any_case() {
        let lower: InfoHash = HEX.parse().unwrap();
        let upper: InfoHash = HEX.to_ascii_uppercase().parse().unwrap();
        assert_eq!(lower, upper);
        assert_eq!(upper.to_string(), HEX);
    }

    #[test]
    fn infohash_parses_base32_to_same_value() {
        let hash: InfoHash = HEX.parse().unwrap();
        let b32 = data_encoding::BASE32.encode(hash.as_bytes());
        assert_eq!(b32.len(), 32);
        assert_eq!(b32.parse::<InfoHash>().unwrap(), hash);
        assert_eq!(b32.to_ascii_lowercase().parse::<InfoHash>().unwrap(), hash);
    }

    #[test]
    fn infohash_rejects_bad_input() {
        assert!("abc".parse::<InfoHash>().is_err());
        assert!("z".repeat(40).parse::<InfoHash>().is_err());
    }

    #[test]
    fn infohash_serde_round_trip() {
        let hash: InfoHash = HEX.parse().unwrap();
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(json, format!("\"{HEX}\""));
        assert_eq!(serde_json::from_str::<InfoHash>(&json).unwrap(), hash);
    }

    #[test]
    fn category_and_sort_parse_case_insensitively() {
        assert_eq!("Movies".parse::<Category>().unwrap(), Category::Movies);
        assert!("films".parse::<Category>().is_err());
        assert_eq!("SEEDERS".parse::<SortOrder>().unwrap(), SortOrder::Seeders);
    }

    #[test]
    fn query_emptiness() {
        assert!(SearchQuery::text("  ").is_empty());
        assert!(!SearchQuery::text("ubuntu").is_empty());
        let imdb = SearchQuery {
            imdb_id: Some("tt0063350".into()),
            ..SearchQuery::text("")
        };
        assert!(!imdb.is_empty());
    }

    #[test]
    fn download_target_serializes_tagged() {
        let target = DownloadTarget::Magnet("magnet:?xt=urn:btih:abc".into());
        assert_eq!(
            serde_json::to_value(&target).unwrap(),
            serde_json::json!({ "type": "magnet", "url": "magnet:?xt=urn:btih:abc" })
        );
    }
}
