//! Serves the console from inside the binary.
//!
//! The console is exported to plain files by `next build` and embedded at compile
//! time, so a deployment is a single executable: no Node runtime, no `node_modules`,
//! and no static file tree to keep in step with the binary.
//!
//! Two behaviours matter and are easy to get wrong:
//!
//! * **Deep links must work.** Every route is a client component that fetches on
//!   mount, so a pasted `/jobs/<uuid>` is resolved by the browser. The export
//!   cannot enumerate UUIDs, so it emits one shell per dynamic route at
//!   `out/<route>/__static_export__.html`. Any unmatched path that looks like a
//!   route falls back to the shell for its nearest known ancestor, so a hard
//!   refresh on a deep link boots the console instead of 404ing.
//! * **Caching must not lie.** `/_next/static/**` assets are content-hashed and
//!   immutable; HTML is not, and must be revalidated or a deploy leaves stale
//!   markup that points at chunks which no longer exist.

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;
use std::sync::OnceLock;

/// The exported console, compiled into the binary.
///
/// `debug-embed` is deliberately off: the assets are already in the binary in
/// release, and embedding them a second time in a debug build makes a slow build
/// slower for no benefit.
///
/// `no-folder-embed` is off because the assets must be in release builds too.
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../forge-web/out"]
struct Assets;

/// Whether a path was embedded at all.
///
/// The check exists because of a real failure: rust-embed treats a `[...]`
/// segment as a glob character class, so Next's `_next/static/chunks/app/jobs/
/// [id]/page-*.js` files were silently skipped. Every other asset was present, so
/// the console looked almost entirely healthy and the five detail pages 404'd on
/// their own script - which reads as a broken export rather than a missing asset.
///
/// The test uses this to assert the bracket-named chunks are embedded, so the
/// regression cannot come back unnoticed.
pub fn has_asset(path: &str) -> bool {
    Assets::get(path).is_some()
}

/// The shell emitted for each dynamic route by the export.
///
/// One per route: `/jobs/<uuid>` and `/executions/<uuid>` need different shells,
/// because each boots a different client bundle. Routes whose shell is missing
/// are filtered out, so a partially exported console still serves what it has.
fn shells() -> &'static [(&'static str, &'static [u8])] {
    static SHELLS: OnceLock<Vec<(&'static str, &'static [u8])>> = OnceLock::new();
    SHELLS.get_or_init(|| {
        ["jobs", "executions", "workers", "incidents", "workflows"]
            .into_iter()
            .filter_map(|route| {
                let file = format!("{route}/__static_export__.html");
                Assets::get(&file).map(|asset| (route, leak(asset.data)))
            })
            .collect()
    })
}

/// Promotes borrowed asset data to `'static` so it can live in a `OnceLock`.
///
/// The assets live for the life of the process - they are compiled into the
/// binary - so this is a memory leak in the strict sense and a non-leak in every
/// sense that matters. `Box::leak` is the honest way to say that.
fn leak(bytes: std::borrow::Cow<'_, [u8]>) -> &'static [u8] {
    Box::leak(bytes.into_owned().into_boxed_slice())
}

/// How many assets are embedded, for diagnostics and tests.
///
/// Exposed because "the console silently did not get built" is otherwise
/// invisible: every route 404s and the cause is not obvious.
pub fn embedded_asset_count() -> usize {
    <Assets as rust_embed::Embed>::iter().count()
}

/// The name of one embedded asset with the given extension, relative to
/// `_next/static/`.
///
/// Used by tests to assert that a real asset is served correctly, rather than
/// inventing a path that may not exist and passing for the wrong reason.
pub fn sample_asset(extension: &str) -> Option<String> {
    Assets::iter().find_map(|file| {
        let name = file.as_ref();
        name.ends_with(&format!(".{extension}"))
            .then(|| name.trim_start_matches("_next/static/").to_string())
    })
}

/// The name of an embedded asset beneath a bracketed route directory, relative to
/// `_next/static/`.
///
/// Used by tests to assert that Next's `[id]` route chunks are actually
/// embedded and resolvable, which is a separate failure from the percent-decoding
/// one and was masked by it.
pub fn sample_asset_under(segment: &str) -> Option<String> {
    <Assets as rust_embed::Embed>::iter().find_map(|file| {
        let name = file.as_ref();
        name.contains(segment)
            .then(|| name.trim_start_matches("_next/static/").to_string())
    })
}

/// Whether a build found a console at all.
///
/// A Rust-only build (someone who never ran the Node build) should say so plainly
/// rather than 404 every route and look like a broken deployment.
fn has_console() -> bool {
    Assets::get("index.html").is_some()
}

/// Serves the console, refusing to shadow the API.
///
/// This is mounted as the API router's own fallback rather than as a separate
/// service: axum's `fallback_service` *replaces* whatever fallback the router
/// already had, so layering it over the API silently removed the API's JSON 404.
/// A mistyped `/api/**` path then returned this page with a 200, and a JSON
/// client parsing the response failed somewhere unrelated to the real mistake.
///
/// One fallback, which looks at the path first, keeps the two behaviours from
/// being able to clobber each other.
pub async fn serve(uri: Uri) -> Response {
    // Anything under the API prefix is an API request, always. Returning HTML for
    // one is worse than a 404: the caller sent JSON and gets a web page.
    if uri.path().starts_with("/api/") {
        return (
            StatusCode::NOT_FOUND,
            [(
                header::CONTENT_TYPE,
                "application/json; charset=utf-8",
            )],
            format!(
                "{{\"error\":{{\"code\":\"NOT_FOUND\",\"message\":\"no such endpoint\",\"details\":[{{\"field\":\"path\",\"issue\":\"{}\"}}]}}}}",
                uri.path().replace('"', "'")
            ),
        )
            .into_response();
    }

    serve_console(uri).await
}

async fn serve_console(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    // A browser percent-encodes `[` and `]` in a URL, so Next's route chunks -
    // `_next/static/chunks/app/jobs/[id]/page-<hash>.js` - arrive as
    // `jobs/%5Bid%5D/page-<hash>.js`. Every other asset resolved and only the
    // five detail pages 404'd on their own script, which reads as a broken export
    // rather than a lookup that never decoded anything.
    //
    // Decoded once, and only if it changes: a path that already contains a literal
    // `%` must not be decoded twice, or a file genuinely named `100%.js` would be
    // looked up as `100 .js`.
    let decoded = percent_decode(path);
    let path = decoded.as_deref().unwrap_or(path);

    // An exact asset or page wins.
    if let Some(asset) = fetch(path) {
        return serve_asset(path, &asset.data);
    }

    // `/jobs` has no extension and is a real page in the export.
    if let Some(asset) = fetch(&format!("{path}.html")) {
        return serve_asset(&format!("{path}.html"), &asset.data);
    }
    if let Some(asset) = fetch(&format!("{path}/index.html")) {
        return serve_asset(&format!("{path}/index.html"), &asset.data);
    }

    // A request for something with an extension is a missing asset, not a route.
    // Falling back to HTML would return a page for a missing `.js`, which turns a
    // clear 404 in the network tab into a confusing MIME type error.
    if path.rsplit('/').next().is_some_and(|last| last.contains('.')) {
        return not_found();
    }

    // A deep link: `/jobs/<uuid>`. Serve the shell for the route it belongs to.
    if let Some(shell) = shell_for(path) {
        return serve_asset("shell.html", shell);
    }

    // Anything else: the console's own 404 page if the export produced one,
    // otherwise the root shell so the client router can decide.
    if let Some(asset) = fetch("404.html") {
        return serve_asset("404.html", &asset.data);
    }
    if let Some(asset) = fetch("index.html") {
        return serve_asset("index.html", &asset.data);
    }

    not_found()
}

/// Percent-decodes a path, returning `None` when nothing changed or decoding
/// failed.
///
/// A malformed escape must not become a lookup for a different file, so anything
/// that does not decode cleanly is left exactly as it arrived.
fn percent_decode(path: &str) -> Option<String> {
    if !path.contains('%') {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8().ok()?;
    (decoded != path).then(|| decoded.into_owned())
}

/// Finds the shell for a deep link by its first segment.
///
/// `/jobs/abc` and `/jobs/abc/runs` both belong to the `jobs` shell.
fn shell_for(path: &str) -> Option<&'static [u8]> {
    let first = path.split('/').next()?;
    shells()
        .iter()
        .find(|(route, _)| *route == first)
        .map(|(_, data)| data.as_ref())
}

fn fetch(path: &str) -> Option<rust_embed::EmbeddedFile> {
    Assets::get(path)
}

/// Serves bytes with an honest `Content-Type` and cache policy.
fn serve_asset(name: &str, bytes: &[u8]) -> Response {
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CONTENT_LENGTH, bytes.len())
        .status(StatusCode::OK);

    if name.starts_with("_next/static/") {
        response = response.header(
            header::CACHE_CONTROL,
            // Next writes content-hashed filenames under `_next/static/`, so a
            // change of content is a change of URL and this cannot be stale.
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    } else {
        response = response.header(
            header::CACHE_CONTROL,
            // HTML must revalidate: it names the hashed chunks it loads, so
            // caching it across a deploy serves markup pointing at files that no
            // longer exist.
            HeaderValue::from_static("public, max-age=0, must-revalidate"),
        );
    }

    response
        .body(Body::from(bytes.to_vec()))
        .unwrap_or_else(|_| not_found())
}

/// The response when no console is embedded.
///
/// This is what a Rust-only build produces, and it names the cause instead of
/// leaving an operator to wonder why every route 404s.
fn not_found() -> Response {
    if !has_console() {
        return (
            StatusCode::NOT_FOUND,
            [(
                header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            )],
            "No console is embedded in this binary.\n\
             Build it first:  cd forge-web && npm ci && npm run build\n\
             The Rust build embeds forge-web/out, so the export must exist first.\n",
        )
            .into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}