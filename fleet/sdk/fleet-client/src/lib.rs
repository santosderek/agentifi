//! HTTP client for Fleet Core.
//!
//! # Why this crate lives in the SDK repository
//!
//! It sits beside `fleet-protocol` and `fleet-enrollment` because it shares their wire contract.
//! A client that redefined those types in a consumer repository would drift from the server the
//! moment either side changed, and the drift would surface as an opaque HTTP 400 rather than a
//! compile error.
//!
//! # The one thing this crate exists to get right
//!
//! Fleet Core validates every request body as **canonical JSON** (sorted keys, no duplicates,
//! byte-for-byte stable). `reqwest`'s `.json()` serialises in struct-declaration order and is
//! therefore rejected. See [`canonical`] for the full story. This crate makes the correct encoding
//! the only reachable path: every request goes through one private `send`, and no public API
//! accepts pre-serialised bytes.
//!
//! # What this crate will never do
//!
//! * Hold, read, or mint the Fleet **root** private key.
//! * Mint an enrollment authorization. That is signed by the Fleet root and is an operator input.
//! * Transmit a device **private** key. Enrollment sends the public half and proves possession by
//!   answering a challenge.
//!
//! # Example
//!
//! ```no_run
//! # async fn run() -> Result<(), fleet_client::FleetClientError> {
//! use fleet_client::{ClientConfig, FleetClient};
//!
//! let config = ClientConfig::new("https://core.example:8088")?;
//! let client = FleetClient::new(config)?;
//! if client.readyz().await? {
//!     // Fleet Core is ready to serve.
//! }
//! # Ok(())
//! # }
//! ```

pub mod canonical;
pub mod config;
pub mod credentials;
pub mod enrollment;
pub mod error;
pub mod types;

pub use config::{ClientConfig, DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_TIMEOUT, Origin};
pub use enrollment::EnrollmentRequest;
pub use error::{CoreRejection, FleetClientError};
pub use types::{IssueChallengeRequestV1, RegisterCardRequestV1, TaskCommandRequestV1, TaskViewV1};

use fleet_enrollment::{
    A2aTaskSubmissionV1, BoundAgentCardV1, DeviceSignedEnvelopeV1, EnrollmentChallengeV1,
    EnrollmentGrantV1, EnrollmentResponseV1, FleetSignedEnvelopeV1,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Installs the rustls crypto provider exactly once for this process.
///
/// The client is built with `rustls-no-provider`, which refuses to choose a provider implicitly.
/// Without this, the first HTTPS request panics at runtime. Pinning to ring matches the rest of
/// the workspace rather than leaving the choice to feature unification.
fn install_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // A provider installed by another component first is fine; only the first install wins and
        // either outcome leaves a usable provider.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// A typed client for one Fleet Core.
#[derive(Debug, Clone)]
pub struct FleetClient {
    http: reqwest::Client,
    config: ClientConfig,
}

impl FleetClient {
    /// Builds a client for the given configuration.
    pub fn new(config: ClientConfig) -> Result<Self, FleetClientError> {
        install_crypto_provider();
        let http = reqwest::Client::builder()
            .timeout(config.timeout())
            // Redirects are refused outright rather than bounded. A redirect can move an exchange
            // that carries signed device material to an origin the caller never allowlisted, and
            // no Fleet Core endpoint legitimately redirects.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| FleetClientError::InvalidEndpoint(error.to_string()))?;
        Ok(Self { http, config })
    }

    /// The configuration this client was built with.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// THE single transmit path for every request body in this crate.
    ///
    /// Centralising it is what makes the canonical-JSON guarantee structural instead of a
    /// convention: there is no other function that writes a request body, and this one always
    /// encodes through [`canonical::encode`]. Adding a new endpoint cannot accidentally bypass it
    /// without deliberately writing a second transport, which review would catch.
    async fn send<Req, Res>(&self, path: &str, body: &Req) -> Result<Res, FleetClientError>
    where
        Req: Serialize,
        Res: DeserializeOwned,
    {
        let url = format!("{}{path}", self.config.base_url());

        // Checked on the resolved URL, not merely the configured base, so a malformed path cannot
        // move the request to an unapproved origin.
        self.config.check_allowed(&url)?;

        let bytes = canonical::encode(body)?;

        let response = self
            .http
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(bytes)
            .send()
            .await
            .map_err(|error| self.transport_error(path, error))?;

        self.decode(path, response).await
    }

    fn transport_error(&self, path: &str, error: reqwest::Error) -> FleetClientError {
        if error.is_timeout() {
            return FleetClientError::Timeout {
                path: path.to_owned(),
                timeout_ms: u64::try_from(self.config.timeout().as_millis()).unwrap_or(u64::MAX),
            };
        }
        FleetClientError::Transport {
            path: path.to_owned(),
            source: Box::new(error),
        }
    }

    /// Turns a response into either a typed value or a precise error.
    ///
    /// The size bound is enforced before the body is buffered where the server declares a length,
    /// and again while reading when it does not, so a missing or lying `Content-Length` cannot be
    /// used to exhaust memory.
    async fn decode<Res: DeserializeOwned>(
        &self,
        path: &str,
        response: reqwest::Response,
    ) -> Result<Res, FleetClientError> {
        let status = response.status();
        let bytes = self.read_bounded(path, response).await?;

        if !status.is_success() {
            // Core answers `{"error": "<code>"}`. The body is parsed only for that short code and
            // is never surfaced verbatim, so an error path cannot leak echoed request content.
            let code = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|value| {
                    value
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                });
            return Err(FleetClientError::Rejected(CoreRejection {
                status: status.as_u16(),
                code,
                path: path.to_owned(),
            }));
        }

        serde_json::from_slice(&bytes).map_err(|error| FleetClientError::Decode {
            path: path.to_owned(),
            message: error.to_string(),
        })
    }

    /// THE single path by which a response body is read.
    ///
    /// Both size checks live here rather than in the callers. An earlier arrangement put the
    /// `Content-Length` check in `decode` only, which silently left `metrics()` -- a different
    /// caller -- enforcing the streaming bound alone; a server declaring a huge length was then
    /// reported as an opaque transport error instead of the accurate `ResponseTooLarge`. Keeping
    /// both checks at the chokepoint is the same discipline the request path uses for canonical
    /// encoding: make the guarantee structural, not a thing each caller must remember.
    async fn read_bounded(
        &self,
        path: &str,
        mut response: reqwest::Response,
    ) -> Result<Vec<u8>, FleetClientError> {
        let limit = self.config.max_response_bytes();

        // Declared length first: refuse before reading a byte when the server is honest about
        // sending something oversized.
        if let Some(length) = response.content_length()
            && length > limit
        {
            return Err(FleetClientError::ResponseTooLarge {
                path: path.to_owned(),
                limit,
            });
        }

        // Then enforce while streaming, so a missing or dishonest header cannot get past it.
        let mut buffer: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| self.transport_error(path, error))?
        {
            if buffer.len() as u64 + chunk.len() as u64 > limit {
                return Err(FleetClientError::ResponseTooLarge {
                    path: path.to_owned(),
                    limit,
                });
            }
            buffer.extend_from_slice(&chunk);
        }
        Ok(buffer)
    }

    async fn probe(&self, path: &str) -> Result<bool, FleetClientError> {
        let url = format!("{}{path}", self.config.base_url());
        self.config.check_allowed(&url)?;
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|error| self.transport_error(path, error))?;
        Ok(response.status().is_success())
    }

    // ---- health ---------------------------------------------------------------------------

    /// `GET /healthz`. True when Fleet Core's process is serving.
    pub async fn healthz(&self) -> Result<bool, FleetClientError> {
        self.probe("/healthz").await
    }

    /// `GET /readyz`. True when Fleet Core attests its store and pinned root.
    ///
    /// Distinct from [`Self::healthz`]: a Core can be alive but not ready, and treating the two as
    /// interchangeable is how a caller ends up sending work to a Core that cannot persist it.
    pub async fn readyz(&self) -> Result<bool, FleetClientError> {
        self.probe("/readyz").await
    }

    /// `GET /metrics`. Prometheus text exposition.
    pub async fn metrics(&self) -> Result<String, FleetClientError> {
        let path = "/metrics";
        let url = format!("{}{path}", self.config.base_url());
        self.config.check_allowed(&url)?;
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|error| self.transport_error(path, error))?;
        let status = response.status();
        let bytes = self.read_bounded(path, response).await?;
        if !status.is_success() {
            return Err(FleetClientError::Rejected(CoreRejection {
                status: status.as_u16(),
                code: None,
                path: path.to_owned(),
            }));
        }
        String::from_utf8(bytes).map_err(|error| FleetClientError::Decode {
            path: path.to_owned(),
            message: error.to_string(),
        })
    }

    // ---- enrollment -----------------------------------------------------------------------

    /// `POST /v1/enrollment/challenges`.
    ///
    /// Step one of the two-phase exchange. Sends the device **public** key together with the
    /// operator-supplied, Fleet-signed authorization; receives a challenge to be answered locally.
    pub async fn issue_challenge(
        &self,
        request: &IssueChallengeRequestV1,
    ) -> Result<EnrollmentChallengeV1, FleetClientError> {
        self.send("/v1/enrollment/challenges", request).await
    }

    /// `POST /v1/enrollment/grants`.
    ///
    /// Step two. The response to the challenge proves possession of the device private key without
    /// transmitting it; the reply is a Fleet-signed grant the caller verifies against its own
    /// pinned root.
    pub async fn accept_enrollment(
        &self,
        response: &EnrollmentResponseV1,
    ) -> Result<FleetSignedEnvelopeV1<EnrollmentGrantV1>, FleetClientError> {
        self.send("/v1/enrollment/grants", response).await
    }

    // ---- A2A ------------------------------------------------------------------------------

    /// `POST /v1/a2a/cards`. Registers a device-signed agent card under a Fleet-signed policy.
    pub async fn register_card(
        &self,
        request: &RegisterCardRequestV1,
    ) -> Result<FleetSignedEnvelopeV1<BoundAgentCardV1>, FleetClientError> {
        self.send("/v1/a2a/cards", request).await
    }

    /// `POST /v1/a2a/tasks`. Submits a device-signed task.
    pub async fn submit_task(
        &self,
        submission: &DeviceSignedEnvelopeV1<A2aTaskSubmissionV1>,
    ) -> Result<TaskViewV1, FleetClientError> {
        self.send("/v1/a2a/tasks", submission).await
    }

    /// `POST /v1/a2a/tasks/status`. Reads current task state.
    ///
    /// Core requires the command's operation to be `Status`; a mismatch is answered with
    /// `invalid_request`, which surfaces here as a 400 rejection.
    pub async fn task_status(
        &self,
        request: &TaskCommandRequestV1,
    ) -> Result<TaskViewV1, FleetClientError> {
        self.send("/v1/a2a/tasks/status", request).await
    }

    /// `POST /v1/a2a/tasks/cancel`. Requests cancellation.
    pub async fn cancel_task(
        &self,
        request: &TaskCommandRequestV1,
    ) -> Result<TaskViewV1, FleetClientError> {
        self.send("/v1/a2a/tasks/cancel", request).await
    }

    /// `POST /v1/a2a/tasks/human-input`. Supplies a response to an `input_required` task.
    pub async fn task_human_input(
        &self,
        request: &TaskCommandRequestV1,
    ) -> Result<TaskViewV1, FleetClientError> {
        self.send("/v1/a2a/tasks/human-input", request).await
    }
}
