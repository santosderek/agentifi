//! Wire-contract conformance fixtures for the VENDORED Fleet client.
//!
//! # Why this file exists (it does not exist upstream)
//!
//! `fleet/sdk/` is a vendored copy of a crate maintained in another repository. Vendoring does not
//! remove the need to stay in sync with Fleet Core — it makes divergence **silent**. Fleet Core
//! validates the wire contract at **runtime**: if Core's request shape changes and this copy does
//! not, nothing fails to compile. The only symptom is `400 invalid_request` from a live Core, with
//! no diagnostic identifying the cause.
//!
//! That failure mode is not hypothetical. `reqwest`'s `.json()` serialises struct fields in
//! declaration order, while Core requires canonical JSON with lexicographically sorted keys. Every
//! request from the Vessel client was rejected until it was found, and the symptom was exactly
//! this opaque `invalid_request`.
//!
//! These tests pin the encoded bytes so a contract change fails **here, in Agentifi's own CI**,
//! rather than at runtime against a deployed Fleet Core.
//!
//! # Why these assert against the real encoder
//!
//! Every test calls [`fleet_client::canonical::encode`] — the actual function the transport uses.
//! A test that re-derived canonical bytes itself would keep passing if the encoder regressed, and
//! would therefore be worthless as a drift guard. That is the whole point of the file.
//!
//! # If one of these fails
//!
//! Do not regenerate the fixture to make it green. A failure means one of:
//!
//!   * the canonical encoder regressed (check `canonical.rs`), or
//!   * a vendored wire type changed shape during a re-vendor.
//!
//! Either way, understand what changed on the Fleet side first. See `fleet/sdk/PROVENANCE.md`.

use fleet_client::IssueChallengeRequestV1;
use fleet_client::canonical::encode;
use fleet_enrollment::{
    DevicePublicKeyV1, EnrollmentAuthorizationV1, FleetSignatureV1, FleetSignedEnvelopeV1,
    IdentityBindingV1,
};
use fleet_protocol::{DigestValue, FleetId, FleetNodeId, KeyId, TenantId, VesselId};

/// Fixed, non-secret test vectors. Deliberately hand-written constants rather than anything
/// generated, so the golden bytes below are reproducible on any machine at any time.
fn binding() -> IdentityBindingV1 {
    IdentityBindingV1 {
        tenant_id: TenantId::new("tenant-conformance").expect("tenant id"),
        fleet_id: FleetId::new("fleet-conformance").expect("fleet id"),
        vessel_id: VesselId::new("vessel-conformance").expect("vessel id"),
        fleet_node_id: FleetNodeId::new("node-conformance").expect("node id"),
        key_id: KeyId::new("key-conformance").expect("key id"),
    }
}

/// A deterministic stand-in for a device public key. This is a PUBLIC half only; no private key
/// material appears anywhere in this crate's tests, by design.
fn device_public_key() -> DevicePublicKeyV1 {
    DevicePublicKeyV1 {
        key_id: KeyId::new("key-conformance").expect("key id"),
        ed25519_public_key: vec![7u8; 32],
    }
}

fn digest() -> DigestValue {
    DigestValue::new(format!("sha256:{}", "ab".repeat(32))).expect("digest")
}

fn issue_challenge_request() -> IssueChallengeRequestV1 {
    IssueChallengeRequestV1 {
        binding: binding(),
        public_key: device_public_key(),
        // Operator-supplied and Fleet-root-signed. This crate never mints one; the value here is
        // a structurally valid placeholder so the OUTER request shape can be pinned.
        authorization: FleetSignedEnvelopeV1 {
            payload: EnrollmentAuthorizationV1 {
                binding: binding(),
                public_key: device_public_key(),
                issued_at_ms: 1_000,
                expires_at_ms: 61_000,
            },
            payload_digest: digest(),
            signature: FleetSignatureV1 {
                key_id: KeyId::new("fleet-root-conformance").expect("root key id"),
                signature: vec![9u8; 64],
            },
        },
    }
}

/// GOLDEN FIXTURE: the exact bytes Fleet Core must receive for `POST /v1/enrollment/challenges`.
///
/// Note every object's keys are in lexicographic order, at every depth. That ordering IS the
/// contract; declaration-order serialisation produces different bytes and Core rejects them.
const GOLDEN_ISSUE_CHALLENGE: &str = concat!(
    r#"{"authorization":{"payload":{"binding":{"fleet_id":"fleet-conformance","#,
    r#""fleet_node_id":"node-conformance","key_id":"key-conformance","#,
    r#""tenant_id":"tenant-conformance","vessel_id":"vessel-conformance"},"#,
    r#""expires_at_ms":61000,"issued_at_ms":1000,"#,
    r#""public_key":{"ed25519_public_key":[7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,"#,
    r#"7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7],"key_id":"key-conformance"}},"#,
    r#""payload_digest":"sha256:abababababababababababababababababababababababababababababababab","#,
    r#""signature":{"key_id":"fleet-root-conformance","#,
    r#""signature":[9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,"#,
    r#"9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9]}},"#,
    r#""binding":{"fleet_id":"fleet-conformance","fleet_node_id":"node-conformance","#,
    r#""key_id":"key-conformance","tenant_id":"tenant-conformance","#,
    r#""vessel_id":"vessel-conformance"},"#,
    r#""public_key":{"ed25519_public_key":[7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,"#,
    r#"7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7],"key_id":"key-conformance"}}"#,
);

/// The load-bearing assertion: the real encoder reproduces the pinned bytes exactly.
#[test]
fn issue_challenge_request_matches_the_golden_wire_bytes() {
    let encoded = encode(&issue_challenge_request()).expect("encodes");
    let actual = String::from_utf8(encoded).expect("utf8");

    assert_eq!(
        actual, GOLDEN_ISSUE_CHALLENGE,
        "the encoded enrollment-challenge request no longer matches the pinned wire contract; \
         see the module docs before touching this fixture"
    );
}

/// Independently confirms the golden bytes satisfy Core's OWN validator.
///
/// Without this, a fixture could be updated to match a broken encoder and the suite would stay
/// green while every real request failed. This anchors the fixture to Fleet Core's definition of
/// canonical rather than to whatever the encoder currently happens to emit.
#[test]
fn the_golden_bytes_are_canonical_by_fleet_cores_own_validator() {
    fleet_protocol::validate_canonical_json_bytes(GOLDEN_ISSUE_CHALLENGE.as_bytes())
        .expect("the pinned fixture must be canonical JSON by Fleet Core's own validator");
}

/// The regression guard for the production bug, restated at the REQUEST level.
///
/// The encoder's own unit tests prove this for a toy struct. This proves it for the actual
/// enrollment request type, which is the one that broke in practice.
#[test]
fn declaration_order_serialisation_of_a_real_request_is_not_canonical() {
    let request = issue_challenge_request();

    let naive = serde_json::to_vec(&request).expect("serialises");
    assert!(
        fleet_protocol::validate_canonical_json_bytes(&naive).is_err(),
        "declaration-order serialisation of a real request must NOT be canonical, or this test \
         proves nothing about the encoder"
    );

    let canonical = encode(&request).expect("encodes");
    assert_ne!(
        naive, canonical,
        "canonical encoding must differ from naive serialisation for this request"
    );
    fleet_protocol::validate_canonical_json_bytes(&canonical)
        .expect("canonical encoding must satisfy Fleet Core's validator");
}

/// Encoding must be deterministic: the same request always produces the same bytes.
///
/// Signatures are computed over these bytes, so instability here would break signature
/// verification intermittently and very confusingly.
#[test]
fn encoding_is_byte_stable_across_repeated_calls() {
    let request = issue_challenge_request();
    let first = encode(&request).expect("encodes");
    for _ in 0..16 {
        assert_eq!(
            first,
            encode(&request).expect("encodes"),
            "canonical encoding must be byte-stable across calls"
        );
    }
}
