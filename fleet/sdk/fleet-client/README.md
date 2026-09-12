# fleet-client

HTTP client for Fleet Core. Lives beside `fleet-protocol` and `fleet-enrollment` because it shares
their wire contract: a client that restated those types in a consumer repository would drift from
the server, and the drift would surface as an opaque `HTTP 400` rather than a compile error.

## The one thing this crate exists to get right

Fleet Core validates **every** request body as canonical JSON — it re-serialises what it received
and requires byte equality, so object keys must be sorted and unique.

`reqwest`'s `.json()` serialises in **struct field order**, so it produces a body Core rejects with
`400 invalid_request`. This is not theoretical: `vessel enroll` was unusable against a live Core
until it was found, and Core's error code gives the client no clue as to the cause.

The guarantee here is **structural, not advisory**:

- `canonical::encode` is the only function that produces a request body.
- `FleetClient::send` (private) is the only function that transmits one.
- No public API accepts pre-serialised bytes, so a caller cannot construct a non-canonical request.
- `encode` re-validates its own output with the **same validator Core runs**, so if the
  `serde_json` key-ordering assumption ever breaks, the client fails locally with a precise message
  instead of emitting a mysterious 400.

## Security posture

| Constraint | How it is met |
|---|---|
| Never holds the Fleet **root** private key | Root public key and the Fleet-signed `EnrollmentAuthorizationV1` are operator-supplied inputs; this crate mints neither |
| Device private key stays with the device | Only the public half is transmitted; possession is proved by signing a challenge through a caller-supplied closure |
| Credentials from files | `credentials::read_private_file` requires a regular, non-symlinked file with no group/other permission bits |
| Endpoint allowlist | Enforced on the **resolved** URL, not just the base; a different port is a different origin |
| No redirect following | `redirect::Policy::none()` — a redirect could move signed material to an unapproved origin |
| Bounded responses | Checked against `Content-Length` *and* while streaming, so a missing or dishonest header cannot defeat it |
| No secrets in logs | Errors carry a path and status, never a request/response body, token or key |
| rustls only | No `native-tls`, no `openssl-sys` (verified with `cargo tree`) |

### TLS trust-store caveat

`reqwest` 0.13 wires its rustls feature to `rustls-platform-verifier`, which reads the OS trust
store, and exposes no `webpki-roots` feature. On a distroless image with no trust store, HTTPS to a
public CA will fail and CA material must be mounted. For the usual Fleet deployment — a private
Core on the cluster network, or HTTPS with an internal CA — this is not hit. Stated here rather
than glossed over.

## Errors

Causes are kept distinguishable, with `is_invalid_request()`, `is_denied()` and `is_retryable()`
helpers so callers need not string-match. This is deliberate: `fleet-migrate` collapsed every
failure into `"migration failed"`, which turned an ordinary missing `GRANT` into a
rebuild-with-instrumentation exercise.

## Usage

```rust,no_run
use fleet_client::{ClientConfig, FleetClient};

# async fn run() -> Result<(), fleet_client::FleetClientError> {
let client = FleetClient::new(ClientConfig::new("https://core.example:8088")?)?;
if client.readyz().await? {
    let view = client.submit_task(&submission).await?;
}
# Ok(())
# }
```

Enrollment sequences both phases, keeping the device key with its owner:

```rust,ignore
let grant = client
    .enroll(&request, |challenge| device.sign(challenge))
    .await?;
// Verify and install `grant` against YOUR pinned trust root. This crate does not,
// because it does not own your trust store.
```

## Testing

All tests run offline against a hand-rolled local mock server; none contacts a live Fleet Core.
They prove canonical encoding *on the wire*, origin refusal, oversized and dishonest-length
response refusal, distinct error classification, timeout behaviour, and that a mismatched
enrollment authorization is caught before anything is transmitted.
