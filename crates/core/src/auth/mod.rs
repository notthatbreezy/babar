//! Authentication helpers.
//!
//! M0 supports cleartext, MD5, and SCRAM-SHA-256. SCRAM-SHA-256-PLUS
//! (channel binding) is deferred to post-v0.1, per `MILESTONES.md`.
//!
//! Each submodule exposes pure (no-I/O) helpers; the driver task drives
//! the conversation.

pub mod md5;
pub mod scram;
