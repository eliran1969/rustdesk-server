//! Serves the embedded web UI (`console-ui/dist`) with an SPA fallback.

use super::error::ApiError;
use axum::{
    body::{boxed, Full},
    http::{header, HeaderValue, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "console-ui/dist/"]
struct Assets;

const INDEX: &str = "index.html";
/// The Vite build loads only same-origin module scripts and stylesheets (no inline
/// scripts); `'unsafe-inline'` styles are allowed for React `style` attributes.
const CSP: &str = concat!(
    "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; ",
    "frame-ancestors 'none'; base-uri 'none'"
);

/// Router fallback: unknown `/api/*` paths get the JSON 404; every other GET/HEAD
/// serves a file from the embedded UI, or `index.html` when no file matches.
pub(crate) async fn serve(method: Method, uri: Uri) -> Response {
    let path = uri.path();
    if path == "/api" || path.starts_with("/api/") {
        return ApiError::NotFound.into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return with_security_headers(StatusCode::METHOD_NOT_ALLOWED.into_response());
    }
    let rel = path.trim_start_matches('/');
    // A missing hashed bundle must fail loudly, not come back as index.html.
    if rel.starts_with("assets/") && !Assets::get(rel).is_some() {
        return with_security_headers(StatusCode::NOT_FOUND.into_response());
    }
    let found = if is_safe_path(rel) {
        Assets::get(rel).map(|f| (rel, f))
    } else {
        None
    };
    let (name, file) = match found.or_else(|| Assets::get(INDEX).map(|f| (INDEX, f))) {
        Some(v) => v,
        None => return with_security_headers(StatusCode::NOT_FOUND.into_response()),
    };
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    let content_type = if mime.type_() == mime_guess::mime::TEXT {
        format!("{}; charset=utf-8", mime.essence_str())
    } else {
        mime.essence_str().to_owned()
    };
    let mut res = Response::new(boxed(Full::from(file.data.into_owned())));
    let headers = res.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control(name)),
    );
    with_security_headers(res)
}

fn is_safe_path(rel: &str) -> bool {
    !rel.is_empty() && !rel.contains('\\') && !rel.split('/').any(|s| s == ".." || s == ".")
}

/// Hashed bundles under `assets/` never change; everything else must be revalidated.
fn cache_control(path: &str) -> &'static str {
    if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

fn with_security_headers(mut res: Response) -> Response {
    let headers = res.headers_mut();
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    res
}

#[cfg(test)]
mod tests {
    use crate::console::test_util::{body_string, test_app};
    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
    };
    use tower::ServiceExt;

    async fn get(uri: &str) -> axum::response::Response {
        let app = test_app().await;
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        app.oneshot(req).await.unwrap()
    }

    #[tokio::test]
    async fn web_serves_index_for_unknown_route() {
        let res = get("/devices").await;
        assert_eq!(res.status(), StatusCode::OK);
        let h = res.headers();
        assert!(h[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/html"));
        assert_eq!(h["x-frame-options"], "DENY");
        assert_eq!(h["x-content-type-options"], "nosniff");
        assert_eq!(h[header::CACHE_CONTROL], "no-cache");
        assert_eq!(
            h[header::CONTENT_SECURITY_POLICY],
            concat!(
                "default-src 'self'; img-src 'self' data:; ",
                "style-src 'self' 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'"
            )
        );
        assert_eq!(h[header::REFERRER_POLICY], "no-referrer");
        let body = body_string(res).await;
        assert!(body.to_ascii_lowercase().contains("<html"), "{body}");
    }

    #[tokio::test]
    async fn web_serves_index_at_root() {
        let res = get("/").await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(body_string(res).await.to_ascii_lowercase().contains("<html"));
    }

    #[tokio::test]
    async fn api_404_not_swallowed_by_spa() {
        let res = get("/api/nope").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert!(res.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("application/json"));
        let body: serde_json::Value = serde_json::from_str(&body_string(res).await).unwrap();
        assert!(body["error"].is_string());
    }

    #[tokio::test]
    async fn web_missing_asset_returns_404() {
        let res = get("/assets/nope.js").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(res.headers()["x-content-type-options"], "nosniff");
        assert!(!body_string(res).await.to_ascii_lowercase().contains("<html"));
    }

    #[tokio::test]
    async fn web_rejects_path_traversal() {
        let res = get("/../Cargo.toml").await;
        let body = body_string(res).await;
        assert!(!body.contains("[package]"));
    }

    #[test]
    fn cache_control_by_path() {
        assert_eq!(super::cache_control("index.html"), "no-cache");
        assert_eq!(
            super::cache_control("assets/index-abc123.js"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(super::cache_control("favicon.svg"), "no-cache");
    }
}
