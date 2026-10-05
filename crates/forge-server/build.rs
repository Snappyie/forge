//! Builds the console and embeds it, so `cargo build` alone produces a single
//! binary.
//!
//! Without this, a clean checkout fails to build: `rust-embed` requires the
//! folder to exist, and the export only exists after `npm run build`. Rather
//! than make that a manual step nobody remembers, this runs the export when the
//! output is missing.
//!
//! Two deliberate behaviours:
//!
//! - **Never rebuilds when the export is present.** The console changes far less
//!   often than Rust code, and re-running a Node build on every `cargo build`
//!   would make the ordinary inner loop several times slower for no reason.
//! - **Fails loudly if Node is missing.** Silently embedding an empty console
//!   would produce a binary that serves 404s for every page and looks like a
//!   deployment problem.

use std::path::Path;
use std::process::Command;

/// The exported console, relative to this crate.
const EXPORT_DIR: &str = "../../forge-web/out";

/// A file the export always produces, used as the existence check.
const SENTINEL: &str = "index.html";

fn main() {
    let export = Path::new(env!("CARGO_MANIFEST_DIR")).join(EXPORT_DIR);

    println!("cargo:rerun-if-changed=../../forge-web/src");
    println!("cargo:rerun-if-changed=../../forge-web/package.json");
    println!("cargo:rerun-if-changed=../../forge-web/package-lock.json");
    println!("cargo:rerun-if-changed=../../forge-web/next.config.ts");

    if export.join(SENTINEL).is_file() {
        // Present, so there is nothing to do. Recompiling on console changes is
        // handled by the explicit `npm run build`, which writes here and then
        // triggers a rebuild via the rerun-if-changed directives above.
        return;
    }

    build_console();
}

/// Runs the Next.js export.
///
/// Failures abort the build: a binary with no console would start, serve 404s for
/// every route, and read as a broken deployment rather than a missing asset.
fn build_console() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../forge-web");

    if !web.join("node_modules").is_dir() {
        // `npm ci` rather than `npm install`: the lockfile is the reproducible
        // set, and a drifting dependency tree would produce a console that does
        // not match the source it was built from.
        run(Command::new("npm").arg("ci").current_dir(&web), "npm ci");
    }

    // No `NEXT_PUBLIC_API_URL`: the export must default to same-origin, so the
    // single binary serves the API and the console from one origin.
    run(
        Command::new("npm").args(["run", "build"]).current_dir(&web),
        "npm run build",
    );
}

/// Runs a command, aborting the build if it fails.
fn run(command: &mut Command, what: &str) {
    let status = command.status();
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("`{what}` failed with {status}"),
        Err(e) => panic!(
            "could not run `{what}`: {e}\n\
             The console is embedded at compile time, so it has to be built first.\n\
             Either install Node, or build the console separately:\n\
             \x20 cd forge-web && npm ci && npm run build\n\
             and then build the Rust workspace."
        ),
    }
}