//! The Forge server as a library.
//!
//! Split out from the binary so the serving layer - the console handler, the
//! embedded assets, the static-file and deep-link rules - can be tested directly
//! rather than by starting a process and making requests over HTTP.

pub mod console;
pub mod runtime;
