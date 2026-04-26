//! TLS transport helpers.

use std::fs::File;
use std::io::BufReader;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;

use crate::auth::scram::ChannelBinding;
use crate::config::{Config, Host, TlsBackend, TlsMode};
use crate::error::{Error, Result};

const SSL_REQUEST_CODE: u32 = 80_877_103;

pub(crate) enum ChannelBindingState {
    Unavailable,
    Failed(String),
    Available(ChannelBinding),
}

pub(crate) struct AnyStream {
    inner: StreamInner,
    channel_binding: ChannelBindingState,
}

enum StreamInner {
    Plain(TcpStream),
    #[cfg(feature = "rustls")]
    Rustls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
    #[cfg(feature = "native-tls")]
    NativeTls(Box<tokio_native_tls::TlsStream<TcpStream>>),
}

impl AnyStream {
    fn plain(stream: TcpStream) -> Self {
        Self {
            inner: StreamInner::Plain(stream),
            channel_binding: ChannelBindingState::Unavailable,
        }
    }

    fn with_channel_binding(inner: StreamInner, channel_binding: ChannelBindingState) -> Self {
        Self {
            inner,
            channel_binding,
        }
    }

    pub(crate) const fn scram_channel_binding(&self) -> &ChannelBindingState {
        &self.channel_binding
    }
}

impl AsyncRead for AnyStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut self.get_mut().inner {
            StreamInner::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            #[cfg(feature = "rustls")]
            StreamInner::Rustls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
            #[cfg(feature = "native-tls")]
            StreamInner::NativeTls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for AnyStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match &mut self.get_mut().inner {
            StreamInner::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            #[cfg(feature = "rustls")]
            StreamInner::Rustls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
            #[cfg(feature = "native-tls")]
            StreamInner::NativeTls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut self.get_mut().inner {
            StreamInner::Plain(stream) => Pin::new(stream).poll_flush(cx),
            #[cfg(feature = "rustls")]
            StreamInner::Rustls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            #[cfg(feature = "native-tls")]
            StreamInner::NativeTls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut self.get_mut().inner {
            StreamInner::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            #[cfg(feature = "rustls")]
            StreamInner::Rustls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
            #[cfg(feature = "native-tls")]
            StreamInner::NativeTls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
        }
    }
}

pub(crate) async fn connect_transport(config: &Config) -> Result<AnyStream> {
    let host = config.host_str();
    let connect_fut = TcpStream::connect((host.as_str(), config.port));
    let stream = match config.connect_timeout {
        Some(timeout) => tokio::time::timeout(timeout, connect_fut)
            .await
            .map_err(|_| {
                Error::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "TCP connect timed out",
                ))
            })?,
        None => connect_fut.await,
    }
    .map_err(Error::Io)?;

    stream.set_nodelay(true).map_err(Error::Io)?;

    match config.tls_mode {
        TlsMode::Disable => Ok(AnyStream::plain(stream)),
        TlsMode::Prefer | TlsMode::Require => negotiate_tls(stream, config).await,
    }
}

async fn negotiate_tls(mut stream: TcpStream, config: &Config) -> Result<AnyStream> {
    let mut request = [0_u8; 8];
    request[..4].copy_from_slice(&8_u32.to_be_bytes());
    request[4..].copy_from_slice(&SSL_REQUEST_CODE.to_be_bytes());
    stream.write_all(&request).await.map_err(Error::Io)?;

    let mut response = [0_u8; 1];
    stream.read_exact(&mut response).await.map_err(Error::Io)?;
    match response[0] {
        b'S' => upgrade_stream(stream, config).await,
        b'N' if config.tls_mode == TlsMode::Prefer => Ok(AnyStream::plain(stream)),
        b'N' => Err(Error::Config(
            "server refused TLS but Config requires it".into(),
        )),
        other => Err(Error::protocol(format!(
            "unexpected SSL negotiation response byte {other:?}"
        ))),
    }
}

async fn upgrade_stream(stream: TcpStream, config: &Config) -> Result<AnyStream> {
    match config.tls_backend {
        TlsBackend::Rustls => upgrade_rustls(stream, config).await,
        TlsBackend::NativeTls => upgrade_native_tls(stream, config).await,
    }
}

fn tls_name(config: &Config) -> Result<String> {
    if let Some(server_name) = config.tls_server_name_str() {
        return Ok(server_name.to_string());
    }

    match &config.host {
        Host::Name(name) => Ok(name.clone()),
        Host::Addr(_) => Err(Error::Config(
            "TLS connections by IP require Config::tls_server_name(...)".into(),
        )),
    }
}

#[cfg(feature = "rustls")]
async fn upgrade_rustls(stream: TcpStream, config: &Config) -> Result<AnyStream> {
    use rustls::pki_types::{CertificateDer, ServerName};
    use rustls::{ClientConfig, RootCertStore};
    use tokio_rustls::TlsConnector;

    let server_name = tls_name(config)?;
    let server_name = ServerName::try_from(server_name)
        .map_err(|_| Error::Config("invalid TLS server name".into()))?;

    let mut roots = RootCertStore::empty();
    let native_certs = rustls_native_certs::load_native_certs();
    for cert in native_certs.certs {
        let _ = roots.add(cert);
    }
    if let Some(path) = config.tls_root_cert_path_ref() {
        let mut reader = BufReader::new(File::open(path).map_err(Error::Io)?);
        let certs = rustls_pemfile::certs(&mut reader)
            .collect::<std::result::Result<Vec<CertificateDer<'static>>, _>>()
            .map_err(|err| Error::Config(format!("failed to read PEM root certificate: {err}")))?;
        for cert in certs {
            roots
                .add(cert)
                .map_err(|err| Error::Config(format!("invalid PEM root certificate: {err}")))?;
        }
    }
    if roots.is_empty() {
        return Err(Error::Config(
            "TLS requested but no trusted root certificates were loaded".into(),
        ));
    }

    let client = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(client));
    let stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|err| Error::Config(format!("TLS handshake failed: {err}")))?;
    let channel_binding = match stream
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certs| certs.first())
        .map(|cert| tls_server_end_point(cert.as_ref()))
        .transpose()
    {
        Ok(Some(binding)) => ChannelBindingState::Available(binding),
        Ok(None) => ChannelBindingState::Unavailable,
        Err(err) => ChannelBindingState::Failed(err.to_string()),
    };
    Ok(AnyStream::with_channel_binding(
        StreamInner::Rustls(Box::new(stream)),
        channel_binding,
    ))
}

#[cfg(not(feature = "rustls"))]
async fn upgrade_rustls(_stream: TcpStream, _config: &Config) -> Result<AnyStream> {
    Err(Error::Config(
        "Config selected rustls TLS, but the `rustls` feature is disabled".into(),
    ))
}

#[cfg(feature = "native-tls")]
async fn upgrade_native_tls(stream: TcpStream, config: &Config) -> Result<AnyStream> {
    use native_tls::{Certificate, TlsConnector};

    let mut builder = TlsConnector::builder();
    if let Some(path) = config.tls_root_cert_path_ref() {
        let cert = std::fs::read(path).map_err(Error::Io)?;
        let cert = Certificate::from_pem(&cert)
            .map_err(|err| Error::Config(format!("failed to read PEM root certificate: {err}")))?;
        builder.add_root_certificate(cert);
    }

    let connector = builder
        .build()
        .map_err(|err| Error::Config(format!("TLS connector build failed: {err}")))?;
    let connector = tokio_native_tls::TlsConnector::from(connector);
    let stream = connector
        .connect(&tls_name(config)?, stream)
        .await
        .map_err(|err| Error::Config(format!("TLS handshake failed: {err}")))?;
    let channel_binding = match stream
        .get_ref()
        .peer_certificate()
        .map_err(|err| Error::Config(format!("failed to read peer certificate: {err}")))?
        .map(|cert| {
            tls_server_end_point(&cert.to_der().map_err(|err| {
                Error::Config(format!("failed to encode peer certificate as DER: {err}"))
            })?)
        })
        .transpose()
    {
        Ok(Some(binding)) => ChannelBindingState::Available(binding),
        Ok(None) => ChannelBindingState::Unavailable,
        Err(err) => ChannelBindingState::Failed(err.to_string()),
    };
    Ok(AnyStream::with_channel_binding(
        StreamInner::NativeTls(Box::new(stream)),
        channel_binding,
    ))
}

#[cfg(not(feature = "native-tls"))]
async fn upgrade_native_tls(_stream: TcpStream, _config: &Config) -> Result<AnyStream> {
    Err(Error::Config(
        "Config selected native-tls, but the `native-tls` feature is disabled".into(),
    ))
}

fn tls_server_end_point(cert_der: &[u8]) -> Result<ChannelBinding> {
    let oid = certificate_signature_algorithm_oid(cert_der)?;
    let digest = match certificate_signature_digest(&oid) {
        Some(CertDigest::Sha224) => Sha224::digest(cert_der).to_vec(),
        Some(CertDigest::Sha256) => Sha256::digest(cert_der).to_vec(),
        Some(CertDigest::Sha384) => Sha384::digest(cert_der).to_vec(),
        Some(CertDigest::Sha512) => Sha512::digest(cert_der).to_vec(),
        None => {
            return Err(Error::Config(format!(
                "unsupported TLS certificate signature algorithm OID for SCRAM channel binding: {}",
                format_oid(&oid)
            )));
        }
    };
    Ok(ChannelBinding::tls_server_end_point(digest))
}

#[derive(Clone, Copy)]
enum CertDigest {
    Sha224,
    Sha256,
    Sha384,
    Sha512,
}

fn certificate_signature_digest(oid: &[u8]) -> Option<CertDigest> {
    if matches_any(
        oid,
        &[
            &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x04],
            &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x05],
            &[0x2a, 0x86, 0x48, 0xce, 0x38, 0x04, 0x03],
            &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x01],
            &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b],
            &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x03, 0x02],
            &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02],
        ],
    ) {
        return Some(CertDigest::Sha256);
    }
    if matches_any(
        oid,
        &[
            &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0e],
            &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x03, 0x01],
            &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x01],
        ],
    ) {
        return Some(CertDigest::Sha224);
    }
    if matches_any(
        oid,
        &[
            &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0c],
            &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x03],
        ],
    ) {
        return Some(CertDigest::Sha384);
    }
    if matches_any(
        oid,
        &[
            &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0d],
            &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x04],
        ],
    ) {
        return Some(CertDigest::Sha512);
    }
    None
}

fn matches_any(oid: &[u8], candidates: &[&[u8]]) -> bool {
    candidates.contains(&oid)
}

fn certificate_signature_algorithm_oid(cert_der: &[u8]) -> Result<Vec<u8>> {
    let cert = read_tlv(cert_der, 0x30)?.0;
    let (_, after_tbs) = read_any(cert)?;
    let algorithm = read_tlv(after_tbs, 0x30)?.0;
    let oid = read_tlv(algorithm, 0x06)?.0;
    Ok(oid.to_vec())
}

fn read_any(input: &[u8]) -> Result<(&[u8], &[u8])> {
    let Some((&tag, rest)) = input.split_first() else {
        return Err(Error::Config(
            "malformed TLS certificate: truncated DER".into(),
        ));
    };
    let (len, body_start) = read_len(rest)?;
    if body_start.len() < len {
        return Err(Error::Config(
            "malformed TLS certificate: DER length exceeds input".into(),
        ));
    }
    let (body, remaining) = body_start.split_at(len);
    let _ = tag;
    Ok((body, remaining))
}

fn read_tlv(input: &[u8], expected_tag: u8) -> Result<(&[u8], &[u8])> {
    let Some((&tag, rest)) = input.split_first() else {
        return Err(Error::Config(
            "malformed TLS certificate: truncated DER".into(),
        ));
    };
    if tag != expected_tag {
        return Err(Error::Config(format!(
            "malformed TLS certificate: expected tag 0x{expected_tag:02x}, got 0x{tag:02x}"
        )));
    }
    let (len, body_start) = read_len(rest)?;
    if body_start.len() < len {
        return Err(Error::Config(
            "malformed TLS certificate: DER length exceeds input".into(),
        ));
    }
    Ok(body_start.split_at(len))
}

fn read_len(input: &[u8]) -> Result<(usize, &[u8])> {
    let Some((&first, rest)) = input.split_first() else {
        return Err(Error::Config(
            "malformed TLS certificate: truncated DER length".into(),
        ));
    };
    if first & 0x80 == 0 {
        return Ok((usize::from(first), rest));
    }

    let octets = usize::from(first & 0x7f);
    if octets == 0 || octets > std::mem::size_of::<usize>() || rest.len() < octets {
        return Err(Error::Config(
            "malformed TLS certificate: invalid DER length".into(),
        ));
    }
    let mut len = 0usize;
    for &byte in &rest[..octets] {
        len = (len << 8) | usize::from(byte);
    }
    Ok((len, &rest[octets..]))
}

fn format_oid(oid: &[u8]) -> String {
    if oid.is_empty() {
        return "<empty>".into();
    }
    let first = oid[0];
    let mut parts = vec![u32::from(first / 40), u32::from(first % 40)];
    let mut value = 0u32;
    for &byte in &oid[1..] {
        value = (value << 7) | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            parts.push(value);
            value = 0;
        }
    }
    parts
        .into_iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_oid() {
        assert_eq!(
            format_oid(&[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b]),
            "1.2.840.113549.1.1.11"
        );
    }
}
