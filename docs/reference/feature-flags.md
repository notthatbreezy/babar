# Cargo features

> Generated rustdoc: <https://docs.rs/babar/latest/babar/index.html>

> See also: [Book Chapter 12 — TLS & security](../book/12-tls.md) and
> [Chapter 10 — Custom codecs](../book/10-custom-codecs.md).

Every feature flag the `babar` crate (and its core crate
`babar-core`) exposes. All features are off by default *except* the
ones listed in `default = [...]`.

## TLS backends

| Feature | What it enables | Default? |
|---|---|---|
| `rustls` | The pure-Rust TLS backend (`TlsBackend::Rustls`). Pulls in `rustls`, `tokio-rustls`, and `rustls-native-certs`. | **yes** |
| `native-tls` | Platform TLS via `native-tls` + `tokio-native-tls` (Schannel / Secure Transport / OpenSSL). Selectable via `TlsBackend::NativeTls`. | no |

Only one TLS backend is needed at runtime; you can enable both if you
want to pick at runtime. `Config::tls_mode(TlsMode::Disable)` opts
out of TLS entirely without touching features.

## Codec features

Each row turns on a codec module under `babar::codec`. Disabling
unused codec features is the most effective way to keep babar's
compile time and binary size small.

| Feature | Codec module | Headline types | Extra deps |
|---|---|---|---|
| `uuid` | `babar::codec::uuid` | `uuid::Uuid` ↔ Postgres `uuid` | `uuid` |
| `time` | `babar::codec::time` | `time::Date` / `Time` / `PrimitiveDateTime` / `OffsetDateTime` | `time` |
| `chrono` | `babar::codec::chrono` | `chrono::NaiveDate` / `NaiveTime` / `NaiveDateTime` / `DateTime<Utc>` | `chrono` |
| `numeric` | `babar::codec::numeric` | `rust_decimal::Decimal` ↔ Postgres `numeric` | `rust_decimal` |
| `json` | `babar::codec::json` | `serde_json::Value` and `typed_json::<T>()` for `Serialize + Deserialize` | `serde`, `serde_json` |
| `array` | `babar::codec::array` | `array(C)` combinator for one-dimensional arrays | `fallible-iterator` |
| `range` | `babar::codec::range` | `range(C)` combinator over discrete and continuous ranges | — |
| `multirange` | `babar::codec::multirange` | `multirange(C)` (Postgres 14+); implies `range` | — |
| `interval` | `babar::codec::interval` | `babar::codec::Interval` | — |
| `net` | `babar::codec::net` | `inet`, `cidr` (`IpAddr`, `Cidr`) | — |
| `macaddr` | `babar::codec::macaddr` | `MacAddr`, `MacAddr8` | — |
| `bits` | `babar::codec::bits` | `BitString` for `bit` / `varbit` | — |
| `hstore` | `babar::codec::hstore` | `Hstore` (`BTreeMap<String, Option<String>>`) | — |
| `citext` | `babar::codec::citext` | `String` ↔ `citext` extension type | — |
| `text-search` | `babar::codec::text_search` | `TsVector`, `TsQuery` | — |
| `pgvector` | `babar::codec::pgvector` | `Vector` for the `pgvector` extension | — |
| `postgis` | `babar::codec::postgis` | `geometry::<T>()` / `geography::<T>()` over `geo-types` | `geo-types` |

Pick what your schema actually uses. A common starting set for an
HTTP service:

```toml
babar = { version = "...", features = ["rustls", "uuid", "time", "json", "numeric"] }
```

## Default features

`default = ["rustls"]`. Disable defaults if you want to ship with
`native-tls`, or with TLS off entirely:

```toml
babar = { version = "...", default-features = false, features = ["native-tls", "uuid"] }
```

## babar-macros

The proc-macro crate (`babar-macros`, exposed via `babar::Codec` and
`babar::sql`) currently exposes no cargo features of its own — it's
unconditionally on when you depend on `babar`.

## Next

For the runtime configuration of TLS, see
[configuration.md](./configuration.md). For Postgres types and the
codec values they map to, see [codecs.md](./codecs.md).
