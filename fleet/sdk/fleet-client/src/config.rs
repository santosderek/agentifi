//! Client configuration: endpoint, origin allowlist, and bounds.

use crate::error::FleetClientError;
use std::collections::BTreeSet;
use std::time::Duration;

/// Default ceiling on a buffered response body.
///
/// Fleet Core's own inbound limit is 96 KiB; responses are grants, cards and task views, all far
/// smaller. 256 KiB leaves generous headroom while still refusing an endpoint that tries to
/// exhaust client memory.
pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 256 * 1024;

/// Default per-request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// A `scheme://host[:port]` origin, normalised for comparison.
///
/// Kept as a distinct type so an allowlist entry cannot be confused with a full URL: allowing
/// `https://core.example/v1/enrollment` would be a category error, and this makes it unspellable.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Origin(String);

impl Origin {
    /// Parses an origin from a URL, discarding any path, query or fragment.
    pub fn parse(url: &str) -> Result<Self, FleetClientError> {
        let url = url.trim();
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| FleetClientError::InvalidEndpoint(format!("{url} has no scheme")))?;
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(FleetClientError::InvalidEndpoint(format!(
                "unsupported scheme {scheme}; only http and https are supported"
            )));
        }
        // Authority ends at the first '/', '?' or '#'.
        let authority = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if authority.is_empty() {
            return Err(FleetClientError::InvalidEndpoint(format!(
                "{url} has no host"
            )));
        }
        // Credentials in the authority are refused rather than silently stripped: a URL carrying
        // `user:password@` is almost always a mistake, and stripping it would send the request
        // somewhere the caller did not intend to authenticate.
        if authority.contains('@') {
            return Err(FleetClientError::InvalidEndpoint(
                "endpoint must not contain embedded credentials".to_owned(),
            ));
        }
        Ok(Self(format!("{scheme}://{authority}")))
    }

    /// The normalised `scheme://host[:port]` string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Origin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// How the client reaches Fleet Core, and what it will accept back.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    base_url: String,
    origin: Origin,
    allowed: BTreeSet<Origin>,
    timeout: Duration,
    max_response_bytes: u64,
}

impl ClientConfig {
    /// Creates a configuration for one Fleet Core base URL.
    ///
    /// The base URL's own origin is allowlisted implicitly; anything else must be added
    /// explicitly with [`Self::allow_origin`]. The allowlist is not optional and cannot be
    /// emptied, so there is no configuration in which the client will talk to an arbitrary host.
    pub fn new(base_url: impl Into<String>) -> Result<Self, FleetClientError> {
        let base_url = base_url.into();
        let origin = Origin::parse(&base_url)?;
        let mut allowed = BTreeSet::new();
        allowed.insert(origin.clone());
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            origin,
            allowed,
            timeout: DEFAULT_TIMEOUT,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        })
    }

    /// Adds an additional permitted origin, for example a failover Core.
    pub fn allow_origin(mut self, url: &str) -> Result<Self, FleetClientError> {
        self.allowed.insert(Origin::parse(url)?);
        Ok(self)
    }

    /// Overrides the per-request timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Overrides the maximum buffered response size.
    #[must_use]
    pub fn with_max_response_bytes(mut self, limit: u64) -> Self {
        self.max_response_bytes = limit;
        self
    }

    /// Confirms a fully-qualified URL is permitted, returning the refused origin otherwise.
    ///
    /// Applied to the resolved request URL rather than only to the base URL, so a path that
    /// somehow escaped the configured origin is still caught before any connection is made.
    pub fn check_allowed(&self, url: &str) -> Result<(), FleetClientError> {
        let origin = Origin::parse(url)?;
        if self.allowed.contains(&origin) {
            Ok(())
        } else {
            Err(FleetClientError::DisallowedOrigin {
                origin: origin.as_str().to_owned(),
            })
        }
    }

    /// Base URL with any trailing slash removed.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Origin of the base URL.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// Configured per-request timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Configured maximum buffered response size.
    pub fn max_response_bytes(&self) -> u64 {
        self.max_response_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_ignores_path_and_case() {
        assert_eq!(
            Origin::parse("HTTPS://Core.Example:8443/v1/a2a/tasks?x=1")
                .expect("parses")
                .as_str(),
            "https://core.example:8443"
        );
    }

    #[test]
    fn origin_rejects_unsupported_scheme_and_embedded_credentials() {
        assert!(Origin::parse("ftp://core.example").is_err());
        assert!(Origin::parse("https://user:pw@core.example").is_err());
        assert!(Origin::parse("core.example").is_err());
    }

    #[test]
    fn base_origin_is_allowed_and_others_are_refused() {
        let config = ClientConfig::new("http://127.0.0.1:8088").expect("config");
        config
            .check_allowed("http://127.0.0.1:8088/v1/a2a/tasks")
            .expect("same origin is allowed");

        let error = config
            .check_allowed("http://evil.example/v1/a2a/tasks")
            .expect_err("different origin must be refused");
        assert!(matches!(error, FleetClientError::DisallowedOrigin { .. }));

        // A different PORT on the same host is a different origin, and must also be refused.
        let error = config
            .check_allowed("http://127.0.0.1:9999/v1/a2a/tasks")
            .expect_err("different port must be refused");
        assert!(matches!(error, FleetClientError::DisallowedOrigin { .. }));
    }

    #[test]
    fn explicitly_allowed_origin_is_accepted() {
        let config = ClientConfig::new("http://127.0.0.1:8088")
            .expect("config")
            .allow_origin("https://failover.example")
            .expect("allow");
        config
            .check_allowed("https://failover.example/healthz")
            .expect("explicitly allowed");
    }
}
