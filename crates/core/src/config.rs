//! Connection configuration.

use std::net::IpAddr;
use std::time::Duration;

/// Configuration for a single Postgres connection.
///
/// Built with the builder methods. Required fields are `host`, `port`,
/// `user`, and `database`; everything else has a default.
///
/// ```
/// use babar::Config;
///
/// let cfg = Config::new("localhost", 5432, "postgres", "postgres")
///     .password("secret")
///     .application_name("smoke-test");
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) host: Host,
    pub(crate) port: u16,
    pub(crate) user: String,
    pub(crate) database: String,
    pub(crate) password: Option<String>,
    pub(crate) application_name: Option<String>,
    pub(crate) connect_timeout: Option<Duration>,
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
        }
    }

    /// Set the password for cleartext, MD5, or SCRAM-SHA-256 authentication.
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

    /// Render the host as a string suitable for `TcpStream::connect`.
    pub(crate) fn host_str(&self) -> String {
        match &self.host {
            Host::Name(s) => s.clone(),
            Host::Addr(a) => a.to_string(),
        }
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
