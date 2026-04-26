//! TLS integration tests.

#![cfg(all(feature = "rustls", unix))]

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use babar::{Config, Error, Session, TlsMode};
use common::DEFAULT_IMAGE;

fn require_tool(name: &str) -> bool {
    Command::new(name)
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn require_docker() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

struct FixtureDir {
    path: PathBuf,
}

impl FixtureDir {
    fn create() -> Self {
        let path = PathBuf::from(format!(
            "/home/chris-brown/projects/babar/crates/core/tests/.tls-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&path).expect("create tls fixture dir");
        Self { path }
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct TlsContainer {
    name: String,
    fixture: FixtureDir,
    port: u16,
}

impl TlsContainer {
    fn start() -> Option<Self> {
        if !require_docker() || !require_tool("openssl") {
            eprintln!("skipping TLS test: docker or openssl unavailable");
            return None;
        }

        let fixture = FixtureDir::create();
        generate_certs(&fixture.path);
        write_hba(&fixture.path);

        let name = format!("babar-tls-{}", rand::random::<u32>());
        let image = std::env::var("BABAR_PG_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_string());
        let output = Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "--name",
                &name,
                "-p",
                "127.0.0.1::5432",
                "-e",
                "POSTGRES_USER=babar",
                "-e",
                "POSTGRES_PASSWORD=secret",
                "-e",
                "POSTGRES_DB=babar",
                "-e",
                "POSTGRES_HOST_AUTH_METHOD=scram-sha-256",
                "-v",
                &format!("{}:/tls-src:ro", fixture.path.display()),
                &image,
                "sh",
                "-ec",
                "mkdir -p /var/lib/postgresql/tls && cp /tls-src/server.crt /var/lib/postgresql/tls/server.crt && cp /tls-src/server.key /var/lib/postgresql/tls/server.key && cp /tls-src/pg_hba.conf /var/lib/postgresql/tls/pg_hba.conf && chown postgres:postgres /var/lib/postgresql/tls/server.crt /var/lib/postgresql/tls/server.key /var/lib/postgresql/tls/pg_hba.conf && chmod 0600 /var/lib/postgresql/tls/server.key && chmod 0644 /var/lib/postgresql/tls/server.crt /var/lib/postgresql/tls/pg_hba.conf && exec docker-entrypoint.sh postgres -c ssl=on -c ssl_cert_file=/var/lib/postgresql/tls/server.crt -c ssl_key_file=/var/lib/postgresql/tls/server.key -c hba_file=/var/lib/postgresql/tls/pg_hba.conf",
            ])
            .output()
            .expect("docker run");
        assert!(
            output.status.success(),
            "docker run failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let port_output = Command::new("docker")
            .args(["port", &name, "5432/tcp"])
            .output()
            .expect("docker port");
        assert!(
            port_output.status.success(),
            "docker port failed: {}",
            String::from_utf8_lossy(&port_output.stderr)
        );
        let stdout = String::from_utf8_lossy(&port_output.stdout);
        let port = stdout
            .lines()
            .find_map(|line| {
                line.split(':')
                    .next_back()
                    .and_then(|value| value.parse().ok())
            })
            .expect("parse port");

        Some(Self {
            name,
            fixture,
            port,
        })
    }

    async fn wait_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last_err = None;
        loop {
            assert!(
                Instant::now() <= deadline,
                "TLS container did not become ready: last error: {:?}\n{}",
                last_err,
                self.logs()
            );
            let cfg = self
                .config()
                .tls_root_cert_path(self.fixture.path.join("ca.crt"));
            match Session::connect(cfg).await {
                Ok(session) => {
                    let _ = session.close().await;
                    return;
                }
                Err(err) => {
                    last_err = Some(err.to_string());
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        }
    }

    fn config(&self) -> Config {
        Config::new("127.0.0.1", self.port, "babar", "babar")
            .password("secret")
            .tls_mode(TlsMode::Require)
            .tls_server_name("localhost")
    }

    fn logs(&self) -> String {
        let output = Command::new("docker")
            .args(["logs", &self.name])
            .output()
            .expect("docker logs");
        format!(
            "STDOUT:\n{}\nSTDERR:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }
}

impl Drop for TlsContainer {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[tokio::test]
async fn rustls_connects_to_tls_only_postgres_with_self_signed_cert() {
    let Some(container) = TlsContainer::start() else {
        return;
    };
    container.wait_ready().await;

    let session = Session::connect(
        container
            .config()
            .tls_root_cert_path(container.fixture.path.join("ca.crt")),
    )
    .await
    .expect("tls connect");
    let rows = session.simple_query_raw("SELECT 1").await.expect("query");
    assert_eq!(rows[0][0][0].as_deref(), Some(&b"1"[..]));
    session.close().await.expect("close");
}

#[tokio::test]
async fn rustls_rejects_untrusted_self_signed_cert() {
    let Some(container) = TlsContainer::start() else {
        return;
    };
    container.wait_ready().await;

    let err = Session::connect(container.config())
        .await
        .expect_err("self-signed cert should not be trusted without root override");
    assert!(matches!(err, Error::Config(message) if message.contains("TLS handshake failed")));
}

fn generate_certs(dir: &Path) {
    let ca_key = dir.join("ca.key");
    let ca_cert = dir.join("ca.crt");
    let key = dir.join("server.key");
    let csr = dir.join("server.csr");
    let cert = dir.join("server.crt");
    let ca_status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-nodes",
            "-newkey",
            "rsa:2048",
            "-sha256",
            "-keyout",
            ca_key.to_str().unwrap(),
            "-out",
            ca_cert.to_str().unwrap(),
            "-days",
            "1",
            "-subj",
            "/CN=babar test ca",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-addext",
            "keyUsage=critical,keyCertSign,cRLSign",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("openssl ca req");
    assert!(
        ca_status.success(),
        "openssl CA certificate generation failed"
    );

    let csr_status = Command::new("openssl")
        .args([
            "req",
            "-nodes",
            "-newkey",
            "rsa:2048",
            "-sha256",
            "-keyout",
            key.to_str().unwrap(),
            "-out",
            csr.to_str().unwrap(),
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("openssl csr req");
    assert!(csr_status.success(), "openssl CSR generation failed");

    fs::write(
        dir.join("server.ext"),
        "basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:localhost\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n",
    )
    .expect("write server extfile");

    let cert_status = Command::new("openssl")
        .args([
            "x509",
            "-req",
            "-in",
            csr.to_str().unwrap(),
            "-CA",
            ca_cert.to_str().unwrap(),
            "-CAkey",
            ca_key.to_str().unwrap(),
            "-CAcreateserial",
            "-out",
            cert.to_str().unwrap(),
            "-days",
            "1",
            "-sha256",
            "-extfile",
            dir.join("server.ext").to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("openssl x509");
    assert!(
        cert_status.success(),
        "openssl server certificate signing failed"
    );

    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).expect("chmod key");
}

fn write_hba(dir: &Path) {
    fs::write(
        dir.join("pg_hba.conf"),
        "local all all trust\nhostnossl all all 0.0.0.0/0 reject\nhostnossl all all ::/0 reject\nhostssl all all 0.0.0.0/0 scram-sha-256\nhostssl all all ::/0 scram-sha-256\n",
    )
    .expect("write pg_hba.conf");
}
