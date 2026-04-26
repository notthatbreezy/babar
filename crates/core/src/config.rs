//! Connection configuration.

use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

/// TLS policy for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    /// Never attempt TLS.
    Disable,
    /// Request TLS and fall back to plain TCP if the server refuses.
    Prefer,
    /// Require TLS. Connecting to a non-TLS server returns an error.
    Require,
}

/// TLS backend used when TLS is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsBackend {
    /// Pure-Rust TLS via `rustls`.
    Rustls,
    /// Platform TLS via `native-tls`.
    NativeTls,
}

/// Configuration for a single Postgres connection.
#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) host: Host,
    pub(crate) port: u16,
    pub(crate) user: String,
    pub(crate) database: String,
    pub(crate) password: Option<String>,
    pub(crate) application_name: Option<String>,
    pub(crate) connect_timeout: Option<Duration>,
    pub(crate) tls_mode: TlsMode,
    pub(crate) tls_backend: TlsBackend,
    pub(crate) tls_server_name: Option<String>,
    pub(crate) tls_root_cert_path: Option<PathBuf>,
}

/// A connection target — either a hostname (resolved at connect time) or a
/// pre-resolved IP address.
#[derive(Debug, Clone)]
pub(crate) enum Host {
    /// Hostname; resolved via [`tokio::net::TcpStream::connect`] at connect time.
    Name(String),
    /// Pre-resolved IP address.
    Addr(IpAddr),
}

impl Config {
    /// Create a config with the four mandatory fields populated.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        database: impl Into<String>,
    ) -> Self {
        Self {
            host: Host::Name(host.into()),
            port,
            user: user.into(),
            database: database.into(),
            password: None,
            application_name: None,
            connect_timeout: None,
            tls_mode: TlsMode::Disable,
            tls_backend: TlsBackend::Rustls,
            tls_server_name: None,
            tls_root_cert_path: None,
        }
    }

    /// Use a pre-resolved IP address as the connection target.
    pub fn with_addr(
        addr: IpAddr,
        port: u16,
        user: impl Into<String>,
        database: impl Into<String>,
    ) -> Self {
        Self {
            host: Host::Addr(addr),
            port,
            user: user.into(),
            database: database.into(),
            password: None,
            application_name: None,
            connect_timeout: None,
            tls_mode: TlsMode::Disable,
            tls_backend: TlsBackend::Rustls,
            tls_server_name: None,
            tls_root_cert_path: None,
        }
    }

    /// Set the password for cleartext, MD5, SCRAM-SHA-256, or
    /// SCRAM-SHA-256-PLUS authentication.
    #[must_use]
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Set the `application_name` startup parameter.
    #[must_use]
    pub fn application_name(mut self, name: impl Into<String>) -> Self {
        self.application_name = Some(name.into());
        self
    }

    /// Set the TCP connect timeout. Default: no timeout (rely on OS).
    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Set the TLS policy.
    #[must_use]
    pub fn tls_mode(mut self, tls_mode: TlsMode) -> Self {
        self.tls_mode = tls_mode;
        self
    }

    /// Convenience shorthand for [`TlsMode::Require`].
    #[must_use]
    pub fn require_tls(self) -> Self {
        self.tls_mode(TlsMode::Require)
    }

    /// Select the TLS backend used when TLS is enabled.
    #[must_use]
    pub fn tls_backend(mut self, tls_backend: TlsBackend) -> Self {
        self.tls_backend = tls_backend;
        self
    }

    /// Override the SNI / certificate name used for TLS validation.
    #[must_use]
    pub fn tls_server_name(mut self, server_name: impl Into<String>) -> Self {
        self.tls_server_name = Some(server_name.into());
        self
    }

    /// Add an extra PEM root certificate used for TLS validation.
    #[must_use]
    pub fn tls_root_cert_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.tls_root_cert_path = Some(path.into());
        self
    }

    /// Render the host as a string suitable for `TcpStream::connect`.
    pub(crate) fn host_str(&self) -> String {
        match &self.host {
            Host::Name(s) => s.clone(),
            Host::Addr(a) => a.to_string(),
        }
    }

    pub(crate) fn tls_server_name_str(&self) -> Option<&str> {
        self.tls_server_name.as_deref()
    }

    pub(crate) fn tls_root_cert_path_ref(&self) -> Option<&PathBuf> {
        self.tls_root_cert_path.as_ref()
    }

    pub(crate) fn user_str(&self) -> &str {
        &self.user
    }

    pub(crate) fn database_str(&self) -> &str {
        &self.database
    }

    pub(crate) fn password_str(&self) -> Option<&str> {
        self.password.as_deref()
    }

    pub(crate) fn application_name_str(&self) -> Option<&str> {
        self.application_name.as_deref()
    }
}
