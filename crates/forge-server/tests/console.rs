//! The single-binary serving layer: console, assets, deep links, and the API.
//!
//! These are the properties that make one executable work, and each one is a
//! failure mode that a "returns 200" check would miss:
//!
//! - an asset must carry the right `Content-Type`, or the browser refuses it
//!   while the console silently renders nothing
//! - hashed assets must be immutable and HTML must not be, or a deploy serves
//!   markup pointing at chunks that no longer exist
//! - a deep link must boot the console, and must boot *its own* route's shell
//!   rather than a generic one
//! - a missing `.js` must be a 404, not HTML, or a typo becomes a MIME error
//! - an unknown `/api/**` path must be JSON, not a web page, or a JSON client
//!   fails on a parse error that points nowhere near the real mistake

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use forge_server::console;
use tower::ServiceExt;

/// The console assets, or `None` when the export has not been built.
///
/// The suite skips rather than fails in that case: a Rust-only checkout has no
/// console to test, and reporting that as a failure would train people to ignore
/// it.
fn assets_present() -> bool {
    console::embedded_asset_count() > 0
}

/// Builds a router shaped like the real one: the API prefix is not mounted here,
/// so `/api/**` reaching the fallback is exactly the case under test.
fn router() -> Router {
    Router::new().fallback(console::serve)
}

async fn get(path: &str) -> axum::response::Response {
    router()
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router response")
}

async fn header_of(path: &str, name: header::HeaderName) -> String {
    get(path)
        .await
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn the_root_serves_the_console() {
    if !assets_present() {
        eprintln!("skipping: the console export is not built (forge-web/out)");
        return;
    }
    let response = get("/").await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4 << 20).await.unwrap();
    let html = String::from_utf8_lossy(&body);
    assert!(
        html.contains("<!DOCTYPE html>") || html.contains("<html"),
        "the root did not serve HTML: {}",
        &html[..html.len().min(200)]
    );
}

#[tokio::test]
async fn an_asset_is_served_with_a_usable_content_type() {
    if !assets_present() {
        return;
    }
    // The entry script the HTML references. Whatever it is, a `.js` served as
    // octet-stream is refused by the browser, so the console renders nothing.
    let entry = console::sample_asset("js").expect("a javascript asset to exist");

    let content_type = header_of(&format!("/_next/static/{entry}"), header::CONTENT_TYPE).await;
    assert!(
        content_type.contains("javascript"),
        "a .js asset was served as {content_type}; the browser refuses it and the \
         console silently renders nothing"
    );

    let response = get(&format!("/_next/static/{entry}")).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn hashed_assets_are_immutable_and_html_is_not() {
    if !assets_present() {
        return;
    }
    let entry = console::sample_asset("js").expect("a javascript asset to exist");

    let asset_cache = header_of(&format!("/_next/static/{entry}"), header::CACHE_CONTROL).await;
    assert!(
        asset_cache.contains("immutable"),
        "a content-hashed asset was served with '{asset_cache}'; a stale cached \
         copy would survive a deploy"
    );

    // HTML names the hashed chunks it loads, so caching it across a deploy serves
    // markup pointing at files that no longer exist.
    let html_cache = header_of("/", header::CACHE_CONTROL).await;
    assert!(
        html_cache.contains("must-revalidate") || html_cache.contains("max-age=0"),
        "HTML was served with '{html_cache}'; a cached copy would point at chunks \
         from a previous deploy"
    );
}

#[tokio::test]
async fn a_deep_link_boots_the_console_rather_than_404ing() {
    if !assets_present() {
        return;
    }
    // The ids are UUIDs, so the export cannot enumerate them. These paths only
    // exist at runtime.
    for path in [
        "/jobs/9c1f0a2e-0000-4000-8000-000000000001",
        "/executions/9c1f0a2e-0000-4000-8000-000000000002",
        "/workers/9c1f0a2e-0000-4000-8000-000000000003",
        "/incidents/9c1f0a2e-0000-4000-8000-000000000004",
        "/workflows/9c1f0a2e-0000-4000-8000-000000000005",
    ] {
        let response = get(path).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "a pasted deep link 404ed: {path}"
        );
    }
}

#[tokio::test]
async fn each_deep_link_boots_its_own_routes_shell() {
    if !assets_present() {
        return;
    }
    // The bug this catches: a fallback that serves one shell for everything. Every
    // route then boots the wrong client bundle and renders the wrong page - which
    // looks like a routing bug in the console, not in the server.
    let mut served: Vec<(&str, String)> = Vec::new();
    for route in ["jobs", "executions", "workers", "incidents", "workflows"] {
        let response = get(&format!("/{route}/some-id")).await;
        let body = axum::body::to_bytes(response.into_body(), 4 << 20)
            .await
            .unwrap();
        served.push((route, String::from_utf8_lossy(&body).to_string()));
    }

    // Compared pairwise rather than through a map keyed by route: `insert`
    // returns the value for the *same* key, so with one entry per route it always
    // returned None and this assertion never ran - the test passed while the
    // server served one shell for every route.
    for (index, (route, html)) in served.iter().enumerate() {
        for (other_route, other_html) in served.iter().skip(index + 1) {
            assert_ne!(
                html, other_html,
                "/{route}/<id> and /{other_route}/<id> served the same shell; the \
                 client router would boot the wrong bundle on one of them"
            );
        }
    }
}

#[tokio::test]
async fn a_percent_encoded_route_chunk_is_served() {
    if !assets_present() {
        return;
    }
    // Next names route chunks after the route, so the detail pages' scripts live
    // under `app/jobs/[id]/`. A browser percent-encodes the brackets, so the
    // server receives `app/jobs/%5Bid%5D/`. Without decoding, every other asset
    // resolves and only the five detail pages 404 on their own script - which
    // looks like a broken export rather than an undecoded lookup.
    let chunk = console::sample_asset_under("[id]").expect("a bracketed route chunk");

    let encoded = chunk.replace('[', "%5B").replace(']', "%5D");
    let response = get(&format!("/_next/static/{encoded}")).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a percent-encoded route chunk 404ed; the browser encodes `[` and `]` in \
         every route chunk path"
    );

    // And the decoded form, which is what a hand-written request sends.
    let decoded = get(&format!("/_next/static/{chunk}")).await;
    assert_eq!(decoded.status(), StatusCode::OK, "the decoded form must work too");
}

#[tokio::test]
async fn a_missing_asset_is_404_and_not_a_page() {
    if !assets_present() {
        return;
    }
    // Falling back to HTML for a missing `.js` turns a clear 404 in the network
    // tab into a MIME type error, and hides the real cause - the file is missing.
    let response = get("/_next/static/chunks/definitely-not-a-real-chunk-9999.js").await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a missing script returned the console's HTML; the browser would report a \
         MIME error instead of a 404"
    );
}

#[tokio::test]
async fn an_unknown_api_path_is_json_and_never_html() {
    if !assets_present() {
        return;
    }
    let response = get("/api/v1/definitely-not-a-real-endpoint").await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "an unknown API path returned {} rather than 404",
        response.status()
    );

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.contains("json"),
        "an unknown API path was served as '{content_type}'; a JSON client would \
         fail on a parse error that points nowhere near the mistyped path"
    );

    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body)
        .expect("the API 404 body must be valid JSON");
    assert_eq!(parsed["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn the_api_prefix_is_never_served_a_page() {
    if !assets_present() {
        return;
    }
    // Every `/api/` path must be refused as JSON, whatever follows it.
    for path in ["/api/", "/api/v1", "/api/nope/deeper", "/api/v1/jobs/abc"] {
        let response = get(path).await;
        assert_ne!(
            response.status(),
            StatusCode::OK,
            "{path} returned 200; the console answered an API request"
        );
    }
}