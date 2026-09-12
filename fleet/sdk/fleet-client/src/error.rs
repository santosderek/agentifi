//! Client errors, deliberately kept distinguishable by cause.
//!
//! Collapsing unrelated failures into one opaque message is a real, previously-paid cost in this
//! system: `fleet-migrate` mapped every failure to the string `"migration failed"`, so an ordinary
//! missing `GRANT` was indistinguishable from a TLS fault or a genuine SQL error and required
//! rebuilding the binary with temporary instrumentation to diagnose. Every variant here therefore
//! names a distinct, actionable cause.

use std::fmt;

/// A rejection reported by Fleet Core itself, as opposed to a transport problem.
///
/// Core answers with `{"error": "<code>"}` and a status; both are preserved because they are not
/// redundant. `invalid_request` (400) means the body was malformed or non-canonical and is a
/// client bug; `denied` (403) means the request was well-formed but refused by policy and is an
/// authorization problem. Conflating them sends an operator looking in the wrong place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreRejection {
    /// HTTP status returned by Fleet Core.
    pub status: u16,
    /// Machine-readable code from Core's error body, when present and parseable.
    pub code: Option<String>,
    /// Request path that was rejected. Never includes the request body.
    pub path: String,
}

impl fmt::Display for CoreRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.code {
            Some(code) => write!(
                formatter,
                "Fleet Core rejected POST {} with HTTP {} ({code})",
                self.path, self.status
            ),
            None => write!(
                formatter,
                "Fleet Core rejected POST {} with HTTP {}",
                self.path, self.status
            ),
        }
    }
}

/// Every way a Fleet client call can fail.
///
/// No variant carries a request body, header value, token or key: these errors are expected to be
/// logged, and a body may contain a signed envelope or device material.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FleetClientError {
    /// The configured base URL could not be parsed, or its scheme/host is unusable.
    #[error("invalid Fleet Core endpoint: {0}")]
    InvalidEndpoint(String),

    /// The endpoint is syntactically valid but not permitted by the configured allowlist.
    ///
    /// Separate from [`Self::InvalidEndpoint`] because the remedy is different: the URL is fine,
    /// the *policy* refused it.
    #[error(
        "endpoint origin {origin} is not in the allowlist; add it explicitly with \
         ClientConfig::allow_origin"
    )]
    DisallowedOrigin {
        /// The `scheme://host[:port]` that was refused.
        origin: String,
    },

    /// The request value could not be encoded as canonical JSON.
    ///
    /// This is a client-side failure that happens *before* any network I/O, so it can never be
    /// confused with a server rejection of a non-canonical body.
    #[error("request could not be encoded as canonical JSON: {0}")]
    CanonicalEncoding(String),

    /// The transport failed: DNS, TCP, TLS, or a timeout. No HTTP status exists.
    #[error("transport failure talking to Fleet Core at {path}: {source}")]
    Transport {
        /// Request path that was attempted.
        path: String,
        /// Underlying transport error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The request timed out.
    #[error("request to {path} timed out after {timeout_ms}ms")]
    Timeout {
        /// Request path that was attempted.
        path: String,
        /// Configured timeout.
        timeout_ms: u64,
    },

    /// Fleet Core answered with a non-success status.
    #[error("{0}")]
    Rejected(CoreRejection),

    /// The response exceeded the configured bound and was refused without being buffered whole.
    #[error("response from {path} exceeds the {limit} byte limit")]
    ResponseTooLarge {
        /// Request path that produced the oversized response.
        path: String,
        /// Configured maximum.
        limit: u64,
    },

    /// The response was received but did not match the expected shape.
    #[error("response from {path} could not be decoded: {message}")]
    Decode {
        /// Request path that produced the undecodable response.
        path: String,
        /// Parser message. Never contains the response body.
        message: String,
    },

    /// A credential file was missing, unreadable, or had unsafe permissions.
    #[error("credential file {path}: {message}")]
    Credential {
        /// Offending path.
        path: String,
        /// What was wrong with it.
        message: String,
    },
}

impl FleetClientError {
    /// True when Fleet Core rejected the request as malformed or non-canonical.
    ///
    /// Exposed so a caller can distinguish "I built a bad request" from "I was denied", without
    /// string-matching on the display form.
    pub fn is_invalid_request(&self) -> bool {
        matches!(
            self,
            Self::Rejected(CoreRejection { status: 400, .. }) | Self::CanonicalEncoding(_)
        )
    }

    /// True when Fleet Core refused a well-formed request on authorization grounds.
    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Rejected(CoreRejection { status: 403, .. }))
    }

    /// True when the failure is a transport-level problem rather than a decision by Core.
    ///
    /// Useful for retry policy: transport failures and 503s may be worth retrying, a 400 never is.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport { .. }
                | Self::Timeout { .. }
                | Self::Rejected(CoreRejection { status: 503, .. })
        )
    }
}
