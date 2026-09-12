//! Transport-level guarantees, proved against a local mock server.
//!
//! No test here contacts a real Fleet Core.

mod support;

use fleet_client::{ClientConfig, FleetClient, FleetClientError};
use fleet_enrollment::{
    DevicePublicKeyV1, EnrollmentChallengeV1, EnrollmentResponseV1, IdentityBindingV1,
};
use fleet_protocol::{FleetId, FleetNodeId, KeyId, TenantId, VesselId};
use support::{Behaviour, MockCore};

const TASK_VIEW_JSON: &str = r#"{"accepted_at_ms":1,"request_digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","state":"submitted","task_digest":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","task_id":"task-1","updated_at_ms":2}"#;

fn client_for(mock: &MockCore) -> FleetClient {
    FleetClient::new(ClientConfig::new(&mock.base_url).expect("config")).expect("client")
}

/// A body the client puts on the wire must satisfy Fleet Core's own canonical validator.
///
/// This is the end-to-end form of the guarantee: not merely that the encoder sorts keys, but that
/// what actually leaves the socket is byte-for-byte what Core will accept. It goes through a real
/// public endpoint method, so it exercises the same private `send` every other call uses.
///
/// `EnrollmentResponseV1` is used because it is the shallowest real request type; the property
/// under test is the encoding, which is type-agnostic.
#[tokio::test]
async fn request_body_on_the_wire_is_canonical_json() {
    let mock = MockCore::start(Behaviour::Respond {
        status: 500,
        body: r#"{"error":"unavailable"}"#.to_owned(),
    })
    .await;
    let client = client_for(&mock);

    let binding = IdentityBindingV1 {
        tenant_id: TenantId::new("tenant-1").expect("tenant"),
        fleet_id: FleetId::new("fleet-1").expect("fleet"),
        vessel_id: VesselId::new("vessel-1").expect("vessel"),
        fleet_node_id: FleetNodeId::new("node-1").expect("node"),
        key_id: KeyId::new("key-1").expect("key"),
    };
    let response = EnrollmentResponseV1 {
        challenge: EnrollmentChallengeV1 {
            binding,
            public_key: DevicePublicKeyV1 {
                key_id: KeyId::new("key-1").expect("key"),
                ed25519_public_key: vec![7u8; 32],
            },
            nonce: vec![1u8; 32],
            issued_at_ms: 1,
            expires_at_ms: 2,
        },
        device_signature: vec![9u8; 64],
    };

    // The response status is irrelevant: the assertion is about what was SENT.
    let _ = client.accept_enrollment(&response).await;

    let captured = mock.captured();
    assert_eq!(captured.len(), 1, "exactly one request should be sent");
    let request = &captured[0];

    fleet_protocol::validate_canonical_json_bytes(&request.body)
        .expect("the body actually transmitted must satisfy Fleet Core's validator");

    // Independently confirm sorted keys at the top level, so this fails loudly if the validator
    // is ever loosened rather than silently passing.
    let text = String::from_utf8(request.body.clone()).expect("utf8");
    assert!(
        text.starts_with(r#"{"challenge":"#),
        "top-level keys must be sorted (challenge before device_signature): {text}"
    );

    assert_eq!(
        request.content_type.as_deref(),
        Some("application/json"),
        "content-type must be set explicitly since the body is posted as raw bytes"
    );
}

/// A request to an origin outside the allowlist is refused before any connection is attempted.
#[tokio::test]
async fn disallowed_origin_is_refused() {
    let mock = MockCore::start(Behaviour::Respond {
        status: 200,
        body: TASK_VIEW_JSON.to_owned(),
    })
    .await;

    // Configure for the mock, then point the base URL at a different origin by building a config
    // whose allowlist does not contain it.
    let config = ClientConfig::new("http://127.0.0.1:1").expect("config");
    let error = config
        .check_allowed(&format!("{}/v1/a2a/tasks", mock.base_url))
        .expect_err("a different port is a different origin and must be refused");

    match error {
        FleetClientError::DisallowedOrigin { origin } => {
            assert!(origin.starts_with("http://127.0.0.1:"));
        }
        other => panic!("expected DisallowedOrigin, got {other:?}"),
    }
}

/// An oversized response body is refused rather than buffered.
#[tokio::test]
async fn oversized_response_is_refused() {
    let mock = MockCore::start(Behaviour::Oversized { bytes: 200_000 }).await;
    let client = FleetClient::new(
        ClientConfig::new(&mock.base_url)
            .expect("config")
            .with_max_response_bytes(4_096),
    )
    .expect("client");

    let error = client.metrics().await.expect_err("must refuse oversized");
    match error {
        FleetClientError::ResponseTooLarge { limit, .. } => assert_eq!(limit, 4_096),
        other => panic!("expected ResponseTooLarge, got {other:?}"),
    }
}

/// A dishonest `Content-Length` must not defeat the bound.
///
/// The header path and the streaming path are separate checks; this pins the header one so a
/// server cannot get a large body through by lying in either direction.
#[tokio::test]
async fn lying_content_length_is_refused() {
    let mock = MockCore::start(Behaviour::LyingContentLength {
        declared: 10_000_000,
        body: "{}".to_owned(),
    })
    .await;
    let client = FleetClient::new(
        ClientConfig::new(&mock.base_url)
            .expect("config")
            .with_max_response_bytes(4_096),
    )
    .expect("client");

    let error = client
        .metrics()
        .await
        .expect_err("declared length over the bound must be refused");
    assert!(matches!(error, FleetClientError::ResponseTooLarge { .. }));
}

/// HTTP 400 and 403 map to distinct, actionable errors rather than one opaque failure.
#[tokio::test]
async fn rejections_are_distinguishable_by_cause() {
    let bad_request = MockCore::start(Behaviour::Respond {
        status: 400,
        body: r#"{"error":"invalid_request"}"#.to_owned(),
    })
    .await;
    let error = client_for(&bad_request)
        .metrics()
        .await
        .expect_err("400 must be an error");
    assert!(
        error.is_invalid_request(),
        "400 must classify as an invalid request, got {error:?}"
    );
    assert!(!error.is_denied(), "400 must not be confused with denial");
    assert!(
        !error.is_retryable(),
        "a malformed request is never retryable"
    );

    let denied = MockCore::start(Behaviour::Respond {
        status: 403,
        body: r#"{"error":"denied"}"#.to_owned(),
    })
    .await;
    let error = client_for(&denied)
        .metrics()
        .await
        .expect_err("403 must be an error");
    assert!(
        error.is_denied(),
        "403 must classify as denied, got {error:?}"
    );
    assert!(!error.is_invalid_request());

    let unavailable = MockCore::start(Behaviour::Respond {
        status: 503,
        body: r#"{"error":"unavailable"}"#.to_owned(),
    })
    .await;
    let error = client_for(&unavailable)
        .metrics()
        .await
        .expect_err("503 must be an error");
    assert!(
        error.is_retryable(),
        "503 must be retryable so callers can back off rather than give up"
    );
}

/// The error message must never echo the response body.
#[tokio::test]
async fn errors_do_not_leak_response_bodies() {
    let mock = MockCore::start(Behaviour::Respond {
        status: 403,
        body: r#"{"error":"denied","secret_token":"super-secret-value"}"#.to_owned(),
    })
    .await;
    let error = client_for(&mock)
        .metrics()
        .await
        .expect_err("403 must be an error");
    let rendered = format!("{error}");
    assert!(
        !rendered.contains("super-secret-value"),
        "error text must not echo the response body: {rendered}"
    );
}

/// A hung server produces a timeout error, not an indefinite stall.
#[tokio::test]
async fn hung_server_times_out() {
    let mock = MockCore::start(Behaviour::Hang).await;
    let client = FleetClient::new(
        ClientConfig::new(&mock.base_url)
            .expect("config")
            .with_timeout(std::time::Duration::from_millis(250)),
    )
    .expect("client");

    let error = client.healthz().await.expect_err("must time out");
    assert!(
        matches!(error, FleetClientError::Timeout { .. }),
        "expected Timeout, got {error:?}"
    );
    assert!(error.is_retryable());
}

/// Health and readiness are reported independently.
#[tokio::test]
async fn health_and_readiness_are_separate_signals() {
    let ready = MockCore::start(Behaviour::Respond {
        status: 200,
        body: String::new(),
    })
    .await;
    assert!(client_for(&ready).healthz().await.expect("probe"));

    let not_ready = MockCore::start(Behaviour::Respond {
        status: 503,
        body: String::new(),
    })
    .await;
    assert!(
        !client_for(&not_ready).readyz().await.expect("probe"),
        "a 503 readiness probe must report not-ready rather than erroring"
    );
}

/// A mismatched operator authorization is caught locally, before any request is sent.
///
/// This matters because Fleet Core answers only `invalid_request`/`denied` and does not say which
/// field was wrong. Catching it here gives the operator the actual reason, and avoids putting a
/// mismatched signed artifact on the wire at all.
#[tokio::test]
async fn mismatched_authorization_is_refused_without_contacting_core() {
    use fleet_client::EnrollmentRequest;
    use fleet_enrollment::{EnrollmentAuthorizationV1, FleetSignatureV1, FleetSignedEnvelopeV1};

    let mock = MockCore::start(Behaviour::Respond {
        status: 200,
        body: TASK_VIEW_JSON.to_owned(),
    })
    .await;
    let client = client_for(&mock);

    let binding = |vessel: &str| IdentityBindingV1 {
        tenant_id: TenantId::new("tenant-1").expect("tenant"),
        fleet_id: FleetId::new("fleet-1").expect("fleet"),
        vessel_id: VesselId::new(vessel).expect("vessel"),
        fleet_node_id: FleetNodeId::new("node-1").expect("node"),
        key_id: KeyId::new("key-1").expect("key"),
    };
    let public_key = DevicePublicKeyV1 {
        key_id: KeyId::new("key-1").expect("key"),
        ed25519_public_key: vec![7u8; 32],
    };

    // The authorization names a DIFFERENT vessel than the one being enrolled.
    let authorization = FleetSignedEnvelopeV1::<EnrollmentAuthorizationV1> {
        payload: EnrollmentAuthorizationV1 {
            binding: binding("some-other-vessel"),
            public_key: public_key.clone(),
            issued_at_ms: 1,
            expires_at_ms: 9_999_999_999_999,
        },
        payload_digest: fleet_protocol::DigestValue::new(format!("sha256:{}", "a".repeat(64)))
            .expect("digest"),
        signature: FleetSignatureV1 {
            key_id: KeyId::new("fleet-root-1").expect("key"),
            signature: vec![3u8; 64],
        },
    };

    let error = client
        .enroll(
            &EnrollmentRequest {
                binding: binding("vessel-1"),
                public_key,
                authorization,
            },
            |_challenge| panic!("the device signer must never be invoked for a bad request"),
        )
        .await
        .expect_err("a mismatched authorization must be refused");

    assert!(
        format!("{error}").contains("does not match"),
        "error should name the mismatch, got: {error}"
    );
    assert!(
        mock.captured().is_empty(),
        "nothing may be transmitted when the local precondition fails"
    );
}
