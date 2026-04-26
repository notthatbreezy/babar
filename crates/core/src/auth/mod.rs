//! Authentication helpers.
//!
//! M0 supports cleartext, MD5, and SCRAM-SHA-256.
//! When TLS exposes certificate channel binding data, the driver upgrades to
//! SCRAM-SHA-256-PLUS automatically.
//!
//! Each submodule exposes pure (no-I/O) helpers; the driver task drives
//! the conversation.

pub mod md5;
pub mod scram;
