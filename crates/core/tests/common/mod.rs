//! Test harness: spawn a Postgres container per test scenario and tear it
//! down on drop.
//!
//! Why hand-rolled rather than `testcontainers-rs`: the integration tests
//! cycle through multiple authentication modes (cleartext, MD5, SCRAM) by
//! setting `POSTGRES_HOST_AUTH_METHOD` differently. Spawning via `docker
//! run` directly is straightforward and keeps this milestone free of
//! version-skew between testcontainers crates.
//!
//! Container lifetime is per `PgContainer` value. Drop kills the container
//! synchronously to make sure no orphans linger when a test panics.

#![allow(dead_code)] // helpers are reused across multiple test files

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use babar::{Config, Session};

/// Default Postgres image used by the test suite. Override with
/// `BABAR_PG_IMAGE` if you want a different version (e.g. `postgres:14`).
pub const DEFAULT_IMAGE: &str = "postgres:17-alpine";

/// Authentication method to configure on the Postgres container.
#[derive(Debug, Clone, Copy)]
pub enum AuthMode {
    /// `password` in `pg_hba.conf` — server requests `cleartext` auth.
    Cleartext,
    /// `md5`.
    Md5,
    /// `scram-sha-256` — Postgres default.
    Scram,
    /// `trust` — no password required (used when the test wants to skip auth).
    Trust,
}

impl AuthMode {
    fn env_value(self) -> &'static str {
        match self {
            AuthMode::Cleartext => "password",
            AuthMode::Md5 => "md5",
            AuthMode::Scram => "scram-sha-256",
            AuthMode::Trust => "trust",
        }
    }
}

/// A running Postgres container. Drop kills it.
pub struct PgContainer {
    name: String,
    port: u16,
    user: String,
    password: String,
    db: String,
}

impl PgContainer {
    /// Image used by the suite (overridable via `BABAR_PG_IMAGE`).
    pub fn image() -> String {
        std::env::var("BABAR_PG_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_string())
    }

    /// Spawn a container with the given auth method. Blocks until the server
    /// accepts a connection or 30s elapses.
    pub async fn start(auth: AuthMode) -> Self {
        Self::start_with_image(auth, Self::image()).await
    }

    /// Spawn a container with an explicit image name. Blocks until the server
    /// accepts a connection or 30s elapses.
    pub async fn start_with_image(auth: AuthMode, image: impl Into<String>) -> Self {
        let suffix = format!("{:x}", rand::random::<u32>());
        let name = format!("babar-test-{suffix}");
        let user = "babar".to_string();
        let password = "secret".to_string();
        let db = "babar".to_string();

        let image = image.into();
        let mut cmd = Command::new("docker");
        cmd.args([
            "run",
            "-d",
            "--rm",
            "--name",
            &name,
            "-p",
            "127.0.0.1::5432",
            "-e",
            &format!("POSTGRES_USER={user}"),
            "-e",
            &format!("POSTGRES_PASSWORD={password}"),
            "-e",
            &format!("POSTGRES_DB={db}"),
            "-e",
            &format!("POSTGRES_HOST_AUTH_METHOD={}", auth.env_value()),
        ]);
        // Force the listed password-encryption to match the auth method:
        // the server will only generate a SCRAM verifier (and thus reject
        // cleartext/md5 logins) unless we override.
        match auth {
            AuthMode::Md5 => {
                cmd.args(["-e", "POSTGRES_INITDB_ARGS=--auth-host=md5"]);
            }
            AuthMode::Cleartext => {
                cmd.args(["-e", "POSTGRES_INITDB_ARGS=--auth-host=password"]);
            }
            AuthMode::Scram | AuthMode::Trust => {}
        }
        cmd.arg(&image).stdout(Stdio::null()).stderr(Stdio::piped());

        let out = cmd.output().expect("docker run");
        assert!(
            out.status.success(),
            "docker run failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        // Find the host port the daemon mapped 5432 to.
        let port_out = Command::new("docker")
            .args(["port", &name, "5432/tcp"])
            .output()
            .expect("docker port");
        if !port_out.status.success() {
            let _ = Command::new("docker").args(["rm", "-f", &name]).output();
            panic!(
                "docker port failed: {}",
                String::from_utf8_lossy(&port_out.stderr)
            );
        }
        let stdout = String::from_utf8_lossy(&port_out.stdout);
        // Output: "0.0.0.0:54321\n[::]:54321\n" — take any IPv4 line.
        let port: u16 = stdout
            .lines()
            .find_map(|l| l.split(':').next_back().and_then(|p| p.trim().parse().ok()))
            .unwrap_or_else(|| panic!("could not parse docker port output: {stdout:?}"));

        let container = PgContainer {
            name,
            port,
            user,
            password,
            db,
        };

        container.wait_ready().await;
        container
    }

    async fn wait_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last_err: Option<babar::Error> = None;
        loop {
            if Instant::now() > deadline {
                let logs = self.logs();
                let err = last_err
                    .as_ref()
                    .map_or_else(|| "<unknown>".to_string(), ToString::to_string);
                panic!(
                    "container {} did not become ready: last error: {err}\nlogs:\n{logs}",
                    self.name
                );
            }
            match Session::connect(self.config(&self.user, &self.password)).await {
                Ok(session) => {
                    let _ = session.close().await;
                    return;
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    }

    fn logs(&self) -> String {
        let out = Command::new("docker")
            .args(["logs", &self.name])
            .output()
            .ok();
        out.map(|o| {
            format!(
                "STDOUT:\n{}\nSTDERR:\n{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr),
            )
        })
        .unwrap_or_default()
    }

    /// Build a [`Config`] targeting this container's mapped port.
    pub fn config(&self, user: &str, password: &str) -> Config {
        Config::new("127.0.0.1", self.port, user, &self.db).password(password)
    }

    /// User name set in `POSTGRES_USER`.
    pub fn user(&self) -> &str {
        &self.user
    }

    /// Password set in `POSTGRES_PASSWORD`.
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Mapped host port for connections.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for PgContainer {
    fn drop(&mut self) {
        // Best-effort kill; --rm means the container also goes away.
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
