//! Connection startup and authentication.
//!
//! Startup is sequential and has no concurrency: the client speaks, the
//! server replies, until we see the first `ReadyForQuery`. We drive it
//! inline (not on the driver task), then split the [`TcpStream`] and hand
//! the halves to the driver.

use std::collections::HashMap;
use std::time::Duration;

use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_util::codec::Decoder;
use tracing::debug;

use crate::auth::md5::md5_password;
use crate::auth::scram::{ScramClient, SCRAM_SHA_256};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::protocol::backend::{AuthRequest, BackendMessage};
use crate::protocol::codec::BackendCodec;
use crate::protocol::frontend;

use super::driver::{spawn, BackendKeyData};
use super::Session;

pub(super) async fn connect(config: Config) -> Result<Session> {
    let host = config.host_str();
    let connect_fut = TcpStream::connect((host.as_str(), config.port));
    let mut stream = match config.connect_timeout {
        Some(t) => timeout(t, connect_fut)
            .await
            .map_err(|_| Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "TCP connect timed out",
            )))?,
        None => connect_fut.await,
    }
    .map_err(Error::Io)?;
    stream.set_nodelay(true).map_err(Error::Io)?;

    let mut io = StartupIo {
        stream: &mut stream,
        rx_buf: BytesMut::with_capacity(1024),
        tx_buf: BytesMut::with_capacity(512),
        codec: BackendCodec,
    };

    // Build and send StartupMessage.
    let mut params: Vec<(&str, &str)> = vec![
        ("user", config.user_str()),
        ("database", config.database_str()),
        ("client_encoding", "UTF8"),
    ];
    if let Some(app) = config.application_name_str() {
        params.push(("application_name", app));
    }
    frontend::startup(params.iter().map(|(k, v)| (*k, *v)), &mut io.tx_buf)?;
    io.flush().await?;

    // Authenticate.
    authenticate(&mut io, &config).await?;

    // Now collect ParameterStatus, BackendKeyData until ReadyForQuery.
    let mut server_params: HashMap<String, String> = HashMap::new();
    let mut key_data: Option<BackendKeyData> = None;
    loop {
        match io.read_message().await? {
            BackendMessage::ParameterStatus { name, value } => {
                server_params.insert(name, value);
            }
            BackendMessage::BackendKeyData { process_id, secret_key } => {
                key_data = Some(BackendKeyData { process_id, secret_key });
            }
            BackendMessage::NoticeResponse { .. } => {
                // Informational; ignore.
            }
            BackendMessage::ReadyForQuery { .. } => break,
            BackendMessage::ErrorResponse { fields } => {
                return Err(server_error(fields));
            }
            other => {
                return Err(Error::protocol(format!(
                    "unexpected message after auth: {other:?}"
                )));
            }
        }
    }

    // Hand the connection off to the driver task.
    let (read, write) = stream.into_split();
    let (tx, params, kd) = spawn(read, write, server_params, key_data).await?;
    Ok(super::new_session(tx, params, kd))
}

async fn authenticate(io: &mut StartupIo<'_>, config: &Config) -> Result<()> {
    loop {
        let msg = io.read_message().await?;
        match msg {
            BackendMessage::Authentication(AuthRequest::Ok) => return Ok(()),
            BackendMessage::Authentication(AuthRequest::CleartextPassword) => {
                let password = config
                    .password_str()
                    .ok_or_else(|| Error::Auth("server requested cleartext password but no password configured".into()))?;
                io.tx_buf.clear();
                frontend::password_message(password, &mut io.tx_buf)?;
                io.flush().await?;
            }
            BackendMessage::Authentication(AuthRequest::Md5Password { salt }) => {
                let password = config
                    .password_str()
                    .ok_or_else(|| Error::Auth("server requested MD5 password but no password configured".into()))?;
                let payload = md5_password(config.user_str(), password, salt);
                io.tx_buf.clear();
                frontend::password_message(&payload, &mut io.tx_buf)?;
                io.flush().await?;
            }
            BackendMessage::Authentication(AuthRequest::SaslMechanisms { mechanisms }) => {
                if !mechanisms.iter().any(|m| m == SCRAM_SHA_256) {
                    return Err(Error::UnsupportedAuth(format!(
                        "server offered {mechanisms:?}; only SCRAM-SHA-256 is supported"
                    )));
                }
                let password = config.password_str().ok_or_else(|| {
                    Error::Auth("server requested SCRAM auth but no password configured".into())
                })?;
                let mut scram = ScramClient::new(password);
                let client_first = scram.client_first()?;
                io.tx_buf.clear();
                frontend::sasl_initial_response(SCRAM_SHA_256, &client_first, &mut io.tx_buf)?;
                io.flush().await?;

                // Server responds with SASLContinue.
                let server_first = match io.read_message().await? {
                    BackendMessage::Authentication(AuthRequest::SaslContinue { data }) => data,
                    BackendMessage::ErrorResponse { fields } => return Err(server_error(fields)),
                    other => return Err(Error::protocol(format!("expected SASLContinue, got {other:?}"))),
                };

                let client_final = scram.client_final(&server_first)?;
                io.tx_buf.clear();
                frontend::sasl_response(&client_final, &mut io.tx_buf)?;
                io.flush().await?;

                // Server responds with SASLFinal.
                let server_final = match io.read_message().await? {
                    BackendMessage::Authentication(AuthRequest::SaslFinal { data }) => data,
                    BackendMessage::ErrorResponse { fields } => return Err(server_error(fields)),
                    other => return Err(Error::protocol(format!("expected SASLFinal, got {other:?}"))),
                };
                scram.verify_server_final(&server_final)?;

                // Then AuthenticationOk follows in the next loop iteration.
            }
            BackendMessage::Authentication(AuthRequest::Unsupported { code }) => {
                return Err(Error::UnsupportedAuth(format!("auth code {code}")));
            }
            BackendMessage::Authentication(AuthRequest::SaslContinue { .. })
            | BackendMessage::Authentication(AuthRequest::SaslFinal { .. }) => {
                return Err(Error::protocol("unsolicited SASL message during initial auth"));
            }
            BackendMessage::ErrorResponse { fields } => return Err(server_error(fields)),
            BackendMessage::ParameterStatus { .. } | BackendMessage::NoticeResponse { .. } => {
                // OK to receive during auth (rare).
            }
            other => {
                return Err(Error::protocol(format!("unexpected message during auth: {other:?}")));
            }
        }
    }
}

fn server_error(fields: Vec<(u8, String)>) -> Error {
    let mut severity = String::new();
    let mut code = String::new();
    let mut message = String::new();
    for (k, v) in fields {
        match k {
            b'S' | b'V' if severity.is_empty() => severity = v,
            b'C' => code = v,
            b'M' => message = v,
            _ => {}
        }
    }
    // Auth failures get rewrapped as Error::Auth so callers can match on it.
    if code == "28P01" || code == "28000" {
        return Error::Auth(format!("{severity} {code}: {message}"));
    }
    Error::Server { severity, code, message }
}

struct StartupIo<'a> {
    stream: &'a mut TcpStream,
    rx_buf: BytesMut,
    tx_buf: BytesMut,
    codec: BackendCodec,
}

impl<'a> StartupIo<'a> {
    async fn flush(&mut self) -> Result<()> {
        self.stream.write_all(&self.tx_buf).await.map_err(Error::Io)?;
        self.tx_buf.clear();
        Ok(())
    }

    async fn read_message(&mut self) -> Result<BackendMessage> {
        loop {
            if let Some(msg) = self.codec.decode(&mut self.rx_buf)? {
                debug!(?msg, "startup recv");
                return Ok(msg);
            }
            // Read more bytes.
            let read = self
                .stream
                .read_buf(&mut self.rx_buf)
                .await
                .map_err(Error::Io)?;
            if read == 0 {
                return Err(Error::Closed);
            }
        }
    }
}

#[allow(dead_code)]
fn _unused(_: Duration) {}
