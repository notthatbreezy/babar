//! Connection startup and authentication.
//!
//! Startup is sequential and has no concurrency: the client speaks, the server
//! replies, until we see the first `ReadyForQuery`. We drive it inline (not on
//! the driver task), then hand the fully-negotiated transport to the driver.

use bytes::BytesMut;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::codec::Decoder;
use tracing::{debug, Instrument as _};

use crate::auth::md5::md5_password;
use crate::auth::scram::{ScramClient, SCRAM_SHA_256, SCRAM_SHA_256_PLUS};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::protocol::backend::{AuthRequest, BackendMessage};
use crate::protocol::codec::BackendCodec;
use crate::protocol::frontend;
use crate::telemetry;
use crate::tls::{self, AnyStream, ChannelBindingState};

use super::driver::{spawn, BackendKeyData};
use super::Session;

pub(super) async fn connect(config: Config, retain_prepared_statements: bool) -> Result<Session> {
    let span = telemetry::connect_span(&config);
    async move {
        let mut stream = tls::connect_transport(&config).await?;

        let mut io = StartupIo {
            stream: &mut stream,
            rx_buf: BytesMut::with_capacity(1024),
            tx_buf: BytesMut::with_capacity(512),
            codec: BackendCodec,
        };

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

        authenticate(&mut io, &config).await?;

        let mut server_params: HashMap<String, String> = HashMap::new();
        let mut key_data: Option<BackendKeyData> = None;
        let initial_transaction_status = loop {
            match io.read_message().await? {
                BackendMessage::ParameterStatus { name, value } => {
                    server_params.insert(name, value);
                }
                BackendMessage::BackendKeyData {
                    process_id,
                    secret_key,
                } => {
                    key_data = Some(BackendKeyData {
                        process_id,
                        secret_key,
                    });
                }
                BackendMessage::NoticeResponse { .. } => {}
                BackendMessage::ReadyForQuery { transaction_status } => break transaction_status,
                BackendMessage::ErrorResponse { fields } => {
                    return Err(Error::from_server_fields(fields));
                }
                other => {
                    return Err(Error::protocol(format!(
                        "unexpected message after auth: {other:?}"
                    )));
                }
            }
        };

        let (tx, params, kd, state) =
            spawn(stream, server_params, key_data, initial_transaction_status).await?;
        Ok(super::new_session(
            tx,
            params,
            kd,
            state,
            retain_prepared_statements,
        ))
    }
    .instrument(span)
    .await
}

async fn authenticate(io: &mut StartupIo<'_>, config: &Config) -> Result<()> {
    loop {
        let msg = io.read_message().await?;
        match msg {
            BackendMessage::Authentication(AuthRequest::Ok) => return Ok(()),
            BackendMessage::Authentication(AuthRequest::CleartextPassword) => {
                let password = config.password_str().ok_or_else(|| {
                    Error::Auth(
                        "server requested cleartext password but no password configured".into(),
                    )
                })?;
                io.tx_buf.clear();
                frontend::password_message(password, &mut io.tx_buf)?;
                io.flush().await?;
            }
            BackendMessage::Authentication(AuthRequest::Md5Password { salt }) => {
                let password = config.password_str().ok_or_else(|| {
                    Error::Auth("server requested MD5 password but no password configured".into())
                })?;
                let payload = md5_password(config.user_str(), password, salt);
                io.tx_buf.clear();
                frontend::password_message(&payload, &mut io.tx_buf)?;
                io.flush().await?;
            }
            BackendMessage::Authentication(AuthRequest::SaslMechanisms { mechanisms }) => {
                let password = config.password_str().ok_or_else(|| {
                    Error::Auth("server requested SCRAM auth but no password configured".into())
                })?;
                let mut scram =
                    match select_scram_client(&mechanisms, io.stream.scram_channel_binding())? {
                        SelectedScram::Plain => ScramClient::new(password),
                        SelectedScram::Plus(binding) => {
                            ScramClient::with_channel_binding(password, Some(binding.clone()))
                        }
                    };
                let client_first = scram.client_first()?;
                io.tx_buf.clear();
                frontend::sasl_initial_response(
                    scram.mechanism_name(),
                    &client_first,
                    &mut io.tx_buf,
                )?;
                io.flush().await?;

                let server_first = match io.read_message().await? {
                    BackendMessage::Authentication(AuthRequest::SaslContinue { data }) => data,
                    BackendMessage::ErrorResponse { fields } => {
                        return Err(Error::from_server_fields(fields));
                    }
                    other => {
                        return Err(Error::protocol(format!(
                            "expected SASLContinue, got {other:?}"
                        )));
                    }
                };

                let client_final = scram.client_final(&server_first)?;
                io.tx_buf.clear();
                frontend::sasl_response(&client_final, &mut io.tx_buf)?;
                io.flush().await?;

                let server_final = match io.read_message().await? {
                    BackendMessage::Authentication(AuthRequest::SaslFinal { data }) => data,
                    BackendMessage::ErrorResponse { fields } => {
                        return Err(Error::from_server_fields(fields));
                    }
                    other => {
                        return Err(Error::protocol(format!(
                            "expected SASLFinal, got {other:?}"
                        )));
                    }
                };
                scram.verify_server_final(&server_final)?;
            }
            BackendMessage::Authentication(AuthRequest::Unsupported { code }) => {
                return Err(Error::UnsupportedAuth(format!("auth code {code}")));
            }
            BackendMessage::Authentication(
                AuthRequest::SaslContinue { .. } | AuthRequest::SaslFinal { .. },
            ) => {
                return Err(Error::protocol(
                    "unsolicited SASL message during initial auth",
                ));
            }
            BackendMessage::ErrorResponse { fields } => {
                return Err(Error::from_server_fields(fields))
            }
            BackendMessage::ParameterStatus { .. } | BackendMessage::NoticeResponse { .. } => {}
            other => {
                return Err(Error::protocol(format!(
                    "unexpected message during auth: {other:?}"
                )));
            }
        }
    }
}

#[derive(Debug)]
enum SelectedScram<'a> {
    Plain,
    Plus(&'a crate::auth::scram::ChannelBinding),
}

fn select_scram_client<'a>(
    mechanisms: &[String],
    channel_binding: &'a ChannelBindingState,
) -> Result<SelectedScram<'a>> {
    if mechanisms
        .iter()
        .any(|mechanism| mechanism == SCRAM_SHA_256_PLUS)
    {
        return match channel_binding {
            ChannelBindingState::Available(binding) => Ok(SelectedScram::Plus(binding)),
            ChannelBindingState::Unavailable => Err(Error::UnsupportedAuth(
                "server offered SCRAM-SHA-256-PLUS but TLS channel binding data is unavailable"
                    .into(),
            )),
            ChannelBindingState::Failed(reason) => Err(Error::UnsupportedAuth(format!(
                "server offered SCRAM-SHA-256-PLUS but channel binding setup failed: {reason}"
            ))),
        };
    }

    if mechanisms
        .iter()
        .any(|mechanism| mechanism == SCRAM_SHA_256)
    {
        return Ok(SelectedScram::Plain);
    }

    Err(Error::UnsupportedAuth(format!(
        "server offered {mechanisms:?}; only SCRAM-SHA-256 and SCRAM-SHA-256-PLUS are supported"
    )))
}

struct StartupIo<'a> {
    stream: &'a mut AnyStream,
    rx_buf: BytesMut,
    tx_buf: BytesMut,
    codec: BackendCodec,
}

impl StartupIo<'_> {
    async fn flush(&mut self) -> Result<()> {
        self.stream
            .write_all(&self.tx_buf)
            .await
            .map_err(Error::Io)?;
        self.tx_buf.clear();
        Ok(())
    }

    async fn read_message(&mut self) -> Result<BackendMessage> {
        loop {
            if let Some(msg) = self.codec.decode(&mut self.rx_buf)? {
                debug!(?msg, "startup recv");
                return Ok(msg);
            }
            let read = self
                .stream
                .read_buf(&mut self.rx_buf)
                .await
                .map_err(Error::Io)?;
            if read == 0 {
                return Err(Error::closed());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::scram::ChannelBinding;

    #[test]
    fn prefers_scram_plus_when_channel_binding_is_available() {
        let mechanisms = vec![SCRAM_SHA_256.to_string(), SCRAM_SHA_256_PLUS.to_string()];
        let binding = ChannelBinding::tls_server_end_point(vec![1, 2, 3]);
        let binding = ChannelBindingState::Available(binding);
        let selected = select_scram_client(&mechanisms, &binding).unwrap();
        assert!(matches!(selected, SelectedScram::Plus(_)));
    }

    #[test]
    fn rejects_scram_plus_when_channel_binding_is_missing() {
        let mechanisms = vec![SCRAM_SHA_256_PLUS.to_string()];
        let err = select_scram_client(&mechanisms, &ChannelBindingState::Unavailable).unwrap_err();
        assert!(matches!(err, Error::UnsupportedAuth(_)));
    }
}
