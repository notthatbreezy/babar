# 12. TLS & security

In this chapter we'll turn TLS on, point at a custom root certificate,
pick a backend, and understand what SCRAM-SHA-256 channel binding
buys us.

## Setup

```rust
use std::path::PathBuf;

use babar::config::{TlsBackend, TlsMode};
use babar::{Config, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let cfg = Config::new("db.example.com", 5432, "postgres", "postgres")
        .password("postgres")
        .application_name("ch12-tls")
        .tls_mode(TlsMode::Require)                                    // type: Config (chained)
        .tls_backend(TlsBackend::Rustls)
        .tls_server_name("db.example.com")
        .tls_root_cert_path(PathBuf::from("/etc/ssl/certs/internal-ca.pem"));

    let session: Session = Session::connect(cfg).await?;               // type: Session
    println!(
        "negotiated TLS — server_version = {}",
        session.params().get("server_version").unwrap_or("?"),
    );
    session.close().await?;
    Ok(())
}
```

## Three modes, pick one

`TlsMode` controls babar's handshake posture:

| `TlsMode` | What babar does |
|---|---|
| `Disable` | Never attempt TLS. Plain TCP. |
| `Prefer` | Ask for TLS; if the server refuses, fall back to plain TCP. |
| `Require` | Demand TLS. A server that refuses is a connection failure. |

For anything outside `localhost`, use `TlsMode::Require`. `Prefer`
is convenient for development against a server you don't control;
it's also the mode an attacker would love your production deploy to
use.

## Two backends, pick one

`TlsBackend::Rustls` is the pure-Rust default; the cargo feature is
`tls-rustls`. `TlsBackend::NativeTls` (cargo feature `native-tls`)
uses the platform's TLS stack (Schannel on Windows, Secure Transport
on macOS, OpenSSL on Linux). Pick `Rustls` unless you have a specific
reason — system roots, FIPS mode, smartcard support — to reach for the
platform `native-tls` stack. See
[reference/feature-flags.md](../reference/feature-flags.md) for the
exact flag names.

## Custom roots

`tls_root_cert_path(path)` reads a PEM bundle from disk and adds
those certificates to the trusted root set for this connection. This
is the right knob for self-signed dev CAs, internal CAs, and
"corporate-root-of-trust"-style deployments. Without it, babar uses
the backend's default root store (system roots for `NativeTls`,
`webpki-roots` for `Rustls`).

`tls_server_name(name)` overrides the SNI hostname babar sends in
the handshake. Useful when you connect by IP but the certificate has a
DNS name; useful when you tunnel through `ssh -L`. Leave it unset
when the connection host already matches the certificate.

## SCRAM-SHA-256 and channel binding

babar speaks Postgres' modern auth handshake, SCRAM-SHA-256, with
optional channel binding when TLS is in play. The short version:

- Your password never crosses the wire — the client and server prove
  knowledge of the salted hash via challenge/response.
- With channel binding (`SCRAM-SHA-256-PLUS`), the proof is bound to
  the TLS channel, so a man-in-the-middle who terminates TLS can't
  reuse the proof against the real server. Postgres advertises
  `SCRAM-SHA-256-PLUS` over TLS connections; babar uses it
  automatically when both sides offer it.

babar also supports MD5 and cleartext-password auth for legacy
servers, but if the server selects something babar doesn't speak —
`gss`, `sspi`, or any auth code babar hasn't implemented — you get
`Error::UnsupportedAuth(_)`. The fix is almost always to update the
server's `pg_hba.conf` to use `scram-sha-256` rather than weakening
the client.

## A "what could go wrong?" checklist

- **`Error::Io(_)` during connect** with TLS on — usually a bad root
  cert, a hostname mismatch, or the server isn't actually serving TLS
  on that port.
- **`Error::UnsupportedAuth(_)`** — server's `pg_hba.conf` selected an
  auth method babar doesn't speak. Switch the role to
  `scram-sha-256`.
- **`Error::Auth(_)`** — wrong password, role can't log in, or
  password expired.
- **`Error::Server { code: "28P01", .. }`** — invalid password, sent
  by the server instead of an `Auth` failure.

## Next

[Chapter 13: Observability](./13-observability.md) zooms out from
TLS to the spans, fields, and logs that make a production-running
babar service legible.
