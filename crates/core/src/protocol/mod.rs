//! Postgres wire protocol message helpers.
//!
//! Thin wrappers over the [`postgres-protocol`] crate's framing, plus a
//! [`tokio_util::codec::Decoder`] for backend messages. Frontend messages
//! are written directly into a [`bytes::BytesMut`] using
//! `postgres-protocol::message::frontend`; we don't add a [`Encoder`] for
//! them because the driver task drives writes by hand.
//!
//! [`postgres-protocol`]: https://docs.rs/postgres-protocol
//! [`Encoder`]: tokio_util::codec::Encoder

pub(crate) mod backend;
pub(crate) mod codec;
pub(crate) mod frontend;
