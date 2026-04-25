//! `babar` — a typed, async Postgres driver for Tokio that speaks the
//! `PostgreSQL` wire protocol directly.
//!
//! This is the M0 surface: enough to connect, authenticate, run a simple
//! query, and shut down cleanly. The public API will broaden in M1.
//!
//! ## Architecture (M0)
//!
//! Every connection is owned by a background driver task. The user holds a
//! [`Session`] which is a thin handle that sends commands over an `mpsc`
//! channel and receives responses on per-command `oneshot` channels. The
//! driver is the sole writer to the socket; user-facing API calls are
//! cancellation-safe by construction.
//!
//! ```text
//!     User code → Session (mpsc handle)
//!                       ↓
//!             Background driver task (owns TcpStream, state machine)
//!                       ↓
//!             Postgres server (wire protocol)
//! ```
//!
//! ## Stability
//!
//! Nothing in this crate is stable yet. The public surface here is what M0
//! ships and is subject to change in every milestone up to v0.1.

#![cfg_attr(docsrs, feature(doc_cfg))]

pub(crate) mod auth;
pub mod codec;
mod config;
mod error;
pub(crate) mod protocol;
pub mod types;

// `tokio::net` is unavailable under `--cfg loom`; the session machinery
// uses TcpStream and so must be cfg-gated. Pure modules above remain
// available so loom tests can import e.g. `babar::Error` if they want.
#[cfg(not(loom))]
mod session;

pub use config::Config;
pub use error::{Error, Result};
#[cfg(not(loom))]
pub use session::{RawRows, ServerParams, Session};
