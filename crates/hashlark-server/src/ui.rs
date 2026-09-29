// SPDX-License-Identifier: GPL-3.0-or-later

//! Serves the web UI (the desktop app's Svelte build) in headless mode.
//!
//! The build is embedded at compile time from `apps/desktop/build` (run
//! `pnpm build` there first). A folder on disk can be served instead.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../apps/desktop/build/"]
#[allow_missing = true]
struct Assets;

/// Where the UI comes from.
#[derive(Debug, Clone)]
pub enum UiSource {
    Embedded,
    Dir(PathBuf),
}

impl UiSource {
    /// Whether an embedded UI exists in this build.
    pub fn embedded_available() -> bool {
        Assets::get("index.html").is_some()
    }

    async fn file(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Self::Embedded => Assets::get(path).map(|f| f.data.into_owned()),
            Self::Dir(dir) => {
                // Refuse anything that could leave the folder.
                if path.split('/').any(|seg| seg == ".." || seg.contains('\\')) {
                    return None;
                }
                tokio::fs::read(dir.join(path)).await.ok()
            }
        }
    }

    /// Answers a request for a UI path, falling back to `index.html` for
    /// client-side routes like `/providers`.
    pub async fn serve(&self, uri: &Uri) -> Response {
        let path = uri.path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };
        if let Some(bytes) = self.file(path).await {
            return asset(path, bytes);
        }
        let looks_like_file = path
            .rsplit('/')
            .next()
            .is_some_and(|last| last.contains('.'));
        if !looks_like_file && let Some(bytes) = self.file("index.html").await {
            return asset("index.html", bytes);
        }
        (StatusCode::NOT_FOUND, "not found").into_response()
    }
}

fn asset(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // Hashed build output never changes; everything else is revalidated.
    let cache = if path.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'",
        )
        .body(Body::from(bytes))
        .expect("valid response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serves_files_and_falls_back_to_index() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<html>app</html>").unwrap();
        std::fs::create_dir_all(dir.path().join("_app/immutable")).unwrap();
        std::fs::write(dir.path().join("_app/immutable/a.js"), "js").unwrap();
        let ui = UiSource::Dir(dir.path().to_path_buf());

        let res = ui.serve(&"/providers".parse().unwrap()).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "text/html");

        let res = ui.serve(&"/_app/immutable/a.js".parse().unwrap()).await;
        assert!(
            res.headers()[header::CACHE_CONTROL]
                .to_str()
                .unwrap()
                .contains("immutable")
        );

        let res = ui.serve(&"/missing.js".parse().unwrap()).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let res = ui.serve(&"/../secret".parse().unwrap()).await;
        assert_ne!(
            res.headers()
                .get(header::CACHE_CONTROL)
                .map(|v| v.to_str().unwrap()),
            Some("immutable")
        );
    }
}
