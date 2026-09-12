//! Vessel -> Fleet synchronisation is device-signed, never Fleet-root-signed.
//!
//! These tests exist to prove a privilege boundary, not merely a happy path. The Fleet root key
//! authorises grants, policy, and cards for the entire Fleet; if it were required to send sync
//! traffic, every edge device would need it. The device-signed path removes that requirement, and
//! the negative tests below are what make the removal safe: each one asserts that some way of
//! smuggling device-signed material into a Fleet-signed position (or vice versa) is rejected.

use fleet_enrollment::*;
use fleet_protocol::*;
use std::collections::{BTreeMap, BTreeSet};

fn binding(key: &str) -> IdentityBindingV1 {
    IdentityBindingV1 {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        fleet_id: FleetId::new("fleet-a").unwrap(),
        vessel_id: VesselId::new("vessel-a").unwrap(),
        fleet_node_id: FleetNodeId::new("node-a").unwrap(),
        key_id: KeyId::new(key).unwrap(),
    }
}

fn root() -> (FleetTrustRootV1, FleetAuthoritySigner) {
    let seed = [7; 32];
    let key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let root = FleetTrustRootV1 {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        fleet_id: FleetId::new("fleet-a").unwrap(),
        key_id: KeyId::new("fleet-root-1").unwrap(),
        ed25519_public_key: key.verifying_key().to_bytes().to_vec(),
    };
    let signer = FleetAuthoritySigner::from_seed(root.clone(), seed).unwrap();
    (root, signer)
}

/// A sync record whose header agrees with `bound`, as `admit_vessel_sync` requires.
fn sync_record(bound: &IdentityBindingV1, record_id: &str) -> FleetRecordV1 {
    FleetRecordV1::new(
        RecordHeaderV1 {
            schema_version: SCHEMA_VERSION_V1,
            record_id: RecordId::new(record_id).unwrap(),
            tenant_id: bound.tenant_id.clone(),
            fleet_id: bound.fleet_id.clone(),
            vessel_id: Some(bound.vessel_id.clone()),
            correlation_id: CorrelationId::new("sync-correlation-1").unwrap(),
            causation_id: None,
            hlc: HybridLogicalClockV1 {
                physical_ms: 12,
                logical: 0,
                node_id: bound.fleet_node_id.clone(),
            },
            local_sequence: 1,
            ownership: OwnershipV1::SharedAppendOnly,
            conflict_state: ConflictStateV1::None,
            provenance: ProvenanceV1 {
                origin_node_id: bound.fleet_node_id.clone(),
                principal_id: PrincipalId::new("vessel-principal").unwrap(),
                trust_domain: "vessel".into(),
                source_record_digest: None,
            },
        },
        FleetRecordPayloadV1::SyncManifest(SyncManifestV1 {
            vessel_id: bound.vessel_id.clone(),
            supported_schema_versions: BTreeSet::from([SCHEMA_VERSION_V1]),
            checkpoints: BTreeMap::new(),
            max_batch_records: 1,
        }),
    )
    .unwrap()
}

/// Enrolls `bound` with `seed` and returns the live authority plus the device keypair.
fn enrolled(
    bound: &IdentityBindingV1,
    seed: [u8; 32],
) -> (FleetEnrollmentAuthorityV1, VesselDeviceKeypairV1) {
    let device = VesselDeviceKeypairV1::from_private_bytes(bound.key_id.clone(), seed);
    let mut authority = FleetEnrollmentAuthorityV1::default();
    let response = device
        .respond(
            authority
                .issue_challenge(bound.clone(), device.public(), 10, 20)
                .unwrap(),
        )
        .unwrap();
    authority.accept_response(response, 11, 1, 100).unwrap();
    (authority, device)
}

/// The whole point: a Vessel can produce admissible sync traffic holding only its own device key.
#[test]
fn device_signed_sync_is_admitted_without_any_fleet_root_key() {
    let (root, _) = root();
    let bound = binding("device-sync-1");
    let (authority, device) = enrolled(&bound, [9; 32]);

    let signed = device
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: bound.clone(),
            record: sync_record(&bound, "sync-record-1"),
        })
        .unwrap();

    assert_eq!(authority.admit_vessel_sync(&signed, &root, 12), Ok(()));
}

/// Tampering with the record after signing must not be admitted.
#[test]
fn tampered_device_signed_sync_record_is_rejected() {
    let (root, _) = root();
    let bound = binding("device-sync-tamper");
    let (authority, device) = enrolled(&bound, [11; 32]);

    let signed = device
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: bound.clone(),
            record: sync_record(&bound, "sync-record-1"),
        })
        .unwrap();

    let mut tampered = signed;
    tampered.payload.record.header.local_sequence = 99;
    // The record carries its own digest, so `record.validate()` catches the edit before the
    // envelope signature is even considered. Either rejection is acceptable; what must never
    // happen is admission.
    assert_eq!(
        authority.admit_vessel_sync(&tampered, &root, 12),
        Err(EnrollmentError::Protocol("record digest mismatch".into()))
    );
}

/// PRIVILEGE BOUNDARY: a device-signed envelope must not be accepted as Fleet-signed.
///
/// Fleet-signed material is the authority for grants and policy. If the purpose string were not
/// mixed into the signed digest, a Vessel could sign `BoundSyncEnvelopeV1` and have it honoured as
/// though the Fleet root had issued it. The structural difference in signature type makes this
/// unrepresentable, and the purpose separation makes the digest itself disagree.
#[test]
fn a_device_signed_envelope_does_not_verify_as_fleet_signed() {
    let (root, _) = root();
    let bound = binding("device-cross-1");
    let (_, device) = enrolled(&bound, [13; 32]);

    let payload = BoundSyncEnvelopeV1 {
        binding: bound.clone(),
        record: sync_record(&bound, "sync-record-1"),
    };
    let device_signed = device.sign_sync(payload.clone()).unwrap();

    // Re-house the device signature in a Fleet-signed envelope, claiming the Fleet root key id.
    let forged = FleetSignedEnvelopeV1 {
        payload: device_signed.payload.clone(),
        payload_digest: device_signed.payload_digest.clone(),
        signature: FleetSignatureV1 {
            key_id: root.key_id.clone(),
            signature: device_signed.signature.clone(),
        },
    };

    // Rejected under the Fleet purpose: the digest was bound to the vessel purpose.
    assert_eq!(
        forged.verify(&root, FLEET_SYNC_ENVELOPE_PURPOSE),
        Err(EnrollmentError::Verification)
    );
    // Rejected under the vessel purpose too: the bytes are not a Fleet-root signature.
    assert_eq!(
        forged.verify(&root, VESSEL_SYNC_ENVELOPE_PURPOSE),
        Err(EnrollmentError::Verification)
    );
}

/// PRIVILEGE BOUNDARY: a Fleet-signed envelope must not be accepted as device-signed.
///
/// Otherwise Fleet could manufacture "the Vessel attested this" records, destroying the per-field
/// authority rule that a Vessel is authoritative for its own local execution facts.
#[test]
fn a_fleet_signed_envelope_does_not_verify_as_device_signed() {
    let (root, authority_signer) = root();
    let bound = binding("device-cross-2");
    let (authority, device) = enrolled(&bound, [15; 32]);

    let fleet_signed = authority_signer
        .sign(
            FLEET_SYNC_ENVELOPE_PURPOSE,
            BoundSyncEnvelopeV1 {
                binding: bound.clone(),
                record: sync_record(&bound, "sync-record-1"),
            },
        )
        .unwrap();

    let recast = DeviceSignedEnvelopeV1 {
        payload: fleet_signed.payload.clone(),
        payload_digest: fleet_signed.payload_digest.clone(),
        signature: fleet_signed.signature.signature.clone(),
    };

    assert_eq!(
        recast.verify(&device.public(), VESSEL_SYNC_ENVELOPE_PURPOSE),
        Err(EnrollmentError::Verification)
    );
    assert_eq!(
        authority.admit_vessel_sync(&recast, &root, 12),
        Err(EnrollmentError::Verification)
    );
}

/// PRIVILEGE BOUNDARY: signing for one purpose must not validate under another.
///
/// Proven directly by signing the exact same payload under an unrelated purpose and showing it
/// fails the sync purpose, which is what stops any other device-signed request type (task submit,
/// card registration) from being replayed as sync traffic.
#[test]
fn an_envelope_signed_for_another_purpose_does_not_validate_as_sync() {
    let (root, _) = root();
    let bound = binding("device-purpose");
    let (authority, device) = enrolled(&bound, [17; 32]);

    let payload = BoundSyncEnvelopeV1 {
        binding: bound.clone(),
        record: sync_record(&bound, "sync-record-1"),
    };

    // Same bytes, different purpose.
    let wrong_purpose = device
        .sign("fleet-agent-card-registration", payload)
        .unwrap();

    assert_eq!(
        wrong_purpose.verify(&device.public(), VESSEL_SYNC_ENVELOPE_PURPOSE),
        Err(EnrollmentError::Verification)
    );
    assert_eq!(
        authority.admit_vessel_sync(&wrong_purpose, &root, 12),
        Err(EnrollmentError::Verification)
    );
}

/// IDENTITY BINDING: device A's envelope cannot be replayed as device B.
///
/// Two enrolled devices in one authority. Relabelling A's envelope to B's binding breaks the
/// digest; leaving the binding intact still resolves to A's key, so B gains nothing.
#[test]
fn a_valid_envelope_from_one_device_cannot_be_replayed_as_another() {
    let (root, _) = root();
    let bound_a = binding("device-a");
    let mut bound_b = binding("device-b");
    bound_b.vessel_id = VesselId::new("vessel-b").unwrap();

    let device_a = VesselDeviceKeypairV1::from_private_bytes(bound_a.key_id.clone(), [19; 32]);
    let device_b = VesselDeviceKeypairV1::from_private_bytes(bound_b.key_id.clone(), [21; 32]);
    let mut authority = FleetEnrollmentAuthorityV1::default();
    for (bound, device) in [(&bound_a, &device_a), (&bound_b, &device_b)] {
        let response = device
            .respond(
                authority
                    .issue_challenge(bound.clone(), device.public(), 10, 20)
                    .unwrap(),
            )
            .unwrap();
        authority.accept_response(response, 11, 1, 100).unwrap();
    }

    let from_a = device_a
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: bound_a.clone(),
            record: sync_record(&bound_a, "sync-record-a"),
        })
        .unwrap();
    assert_eq!(authority.admit_vessel_sync(&from_a, &root, 12), Ok(()));

    // Relabel the signed envelope as if device B had sent it.
    let mut replayed = from_a.clone();
    replayed.payload.binding = bound_b.clone();
    replayed.payload.record = sync_record(&bound_b, "sync-record-a");
    assert_eq!(
        authority.admit_vessel_sync(&replayed, &root, 12),
        Err(EnrollmentError::Verification),
        "device A's signature must not authenticate a payload claiming device B"
    );

    // Swapping only the binding while keeping A's record is likewise rejected.
    let mut mixed = from_a;
    mixed.payload.binding = bound_b;
    assert_eq!(
        authority.admit_vessel_sync(&mixed, &root, 12),
        Err(EnrollmentError::Verification)
    );
}

/// IDENTITY BINDING: the builder refuses to sign for a binding this key does not own.
#[test]
fn the_builder_refuses_a_binding_that_does_not_match_the_signing_key() {
    let bound = binding("device-owned");
    let mut other = binding("device-not-owned");
    other.vessel_id = VesselId::new("vessel-other").unwrap();
    let device = VesselDeviceKeypairV1::from_private_bytes(bound.key_id.clone(), [23; 32]);

    assert_eq!(
        device
            .sign_sync(BoundSyncEnvelopeV1 {
                binding: other.clone(),
                record: sync_record(&other, "sync-record-1"),
            })
            .err(),
        Some(EnrollmentError::KeyDenied)
    );
}

/// KEY LIFECYCLE: a revoked key must not verify, even for an envelope signed while it was valid.
#[test]
fn a_revoked_key_cannot_admit_sync() {
    let (root, _) = root();
    let bound = binding("device-revoked");
    let (mut authority, device) = enrolled(&bound, [25; 32]);

    let signed = device
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: bound.clone(),
            record: sync_record(&bound, "sync-record-1"),
        })
        .unwrap();
    assert_eq!(authority.admit_vessel_sync(&signed, &root, 12), Ok(()));

    authority.revoke(&bound).unwrap();
    assert_eq!(
        authority.admit_vessel_sync(&signed, &root, 12),
        Err(EnrollmentError::AdmissionDenied),
        "revocation must take effect for already-signed envelopes, not just future ones"
    );
}

/// KEY LIFECYCLE: after rotation the superseded key must no longer admit sync.
#[test]
fn a_rotated_key_cannot_admit_sync() {
    let (root, _) = root();
    let bound = binding("device-rotated");
    let (mut authority, device) = enrolled(&bound, [27; 32]);

    let signed = device
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: bound.clone(),
            record: sync_record(&bound, "sync-record-1"),
        })
        .unwrap();
    assert_eq!(authority.admit_vessel_sync(&signed, &root, 12), Ok(()));

    // Enroll a replacement key for the same Vessel/node; the old key is superseded.
    let mut rotated_binding = bound.clone();
    rotated_binding.key_id = KeyId::new("device-rotated-2").unwrap();
    let rotated_device =
        VesselDeviceKeypairV1::from_private_bytes(rotated_binding.key_id.clone(), [29; 32]);
    let response = rotated_device
        .respond(
            authority
                .issue_challenge(rotated_binding.clone(), rotated_device.public(), 12, 22)
                .unwrap(),
        )
        .unwrap();
    authority.accept_response(response, 13, 2, 100).unwrap();

    assert_eq!(
        authority.admit_vessel_sync(&signed, &root, 14),
        Err(EnrollmentError::AdmissionDenied),
        "the superseded key must stop admitting sync once rotated"
    );

    // The replacement key works.
    let signed_new = rotated_device
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: rotated_binding.clone(),
            record: sync_record(&rotated_binding, "sync-record-2"),
        })
        .unwrap();
    assert_eq!(authority.admit_vessel_sync(&signed_new, &root, 14), Ok(()));
}

/// KEY LIFECYCLE: an expired grant must not verify.
#[test]
fn an_expired_grant_cannot_admit_sync() {
    let (root, _) = root();
    let bound = binding("device-expired");
    let (authority, device) = enrolled(&bound, [31; 32]);

    let signed = device
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: bound.clone(),
            record: sync_record(&bound, "sync-record-1"),
        })
        .unwrap();

    assert_eq!(authority.admit_vessel_sync(&signed, &root, 12), Ok(()));
    // The grant issued by `enrolled` expires at 100.
    assert_eq!(
        authority.admit_vessel_sync(&signed, &root, 100),
        Err(EnrollmentError::AdmissionDenied),
        "an envelope must stop being admissible once the grant expires"
    );
    assert_eq!(
        authority.admit_vessel_sync(&signed, &root, 10_000),
        Err(EnrollmentError::AdmissionDenied)
    );
}

/// SCOPE: the signing root's tenant/Fleet scope is still enforced on the device-signed path.
#[test]
fn device_signed_sync_still_requires_the_signing_root_scope() {
    let (root, _) = root();
    let mut cross_fleet = binding("device-cross-fleet");
    cross_fleet.fleet_id = FleetId::new("fleet-b").unwrap();
    let (authority, device) = enrolled(&cross_fleet, [33; 32]);

    let signed = device
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: cross_fleet.clone(),
            record: sync_record(&cross_fleet, "sync-record-1"),
        })
        .unwrap();

    assert_eq!(
        authority.admit_vessel_sync(&signed, &root, 12),
        Err(EnrollmentError::AdmissionDenied),
        "a device enrolled in another Fleet must not sync into this root's scope"
    );
}

/// A record whose header disagrees with the signed binding is rejected.
#[test]
fn a_header_that_disagrees_with_the_binding_is_rejected() {
    let (root, _) = root();
    let bound = binding("device-header");
    let (authority, device) = enrolled(&bound, [35; 32]);

    let mut record = sync_record(&bound, "sync-record-1");
    let elsewhere = FleetNodeId::new("node-elsewhere").unwrap();
    // `FleetRecordV1::new` enforces that the HLC node equals the provenance origin, so both must
    // move together to build a structurally valid record that still disagrees with the binding.
    record.header.provenance.origin_node_id = elsewhere.clone();
    record.header.hlc.node_id = elsewhere;
    // Re-seal so the digest matches; only the header/binding disagreement remains.
    let record = FleetRecordV1::new(record.header, record.payload).unwrap();

    let signed = device
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: bound.clone(),
            record,
        })
        .unwrap();

    assert_eq!(
        authority.admit_vessel_sync(&signed, &root, 12),
        Err(EnrollmentError::AdmissionDenied)
    );
}

/// Non-sync payloads must not ride the sync purpose.
#[test]
fn a_non_sync_payload_is_refused_on_the_sync_purpose() {
    let (root, _) = root();
    let bound = binding("device-payload");
    let (authority, device) = enrolled(&bound, [37; 32]);

    let mut record = sync_record(&bound, "sync-record-1");
    record.payload = FleetRecordPayloadV1::RoomMessage(RoomMessageV1 {
        room_id: RoomId::new("room-a").unwrap(),
        message_id: MessageId::new("message-a").unwrap(),
        author: PrincipalId::new("vessel-principal").unwrap(),
        body: "fixture".into(),
        correction_of: None,
        membership_revision: 1,
    });
    let record = FleetRecordV1::new(record.header, record.payload).unwrap();

    let signed = device
        .sign_sync(BoundSyncEnvelopeV1 {
            binding: bound.clone(),
            record,
        })
        .unwrap();

    assert_eq!(
        authority.admit_vessel_sync(&signed, &root, 12),
        Err(EnrollmentError::UnsignedFixtureDenied)
    );
}

/// DOMAIN SEPARATION: the two sync purposes must be distinct from EACH OTHER.
///
/// The pre-existing cross-purpose test signs with an unrelated third purpose
/// (`fleet-agent-card-registration`), which proves only that *some* other purpose fails. It does
/// not pin the property that actually matters here: that the Fleet->Vessel and Vessel->Fleet sync
/// purposes are different domains. Collapsing the two constants to the same string left that test
/// green, which is why this one exists.
///
/// Key separation (device key vs Fleet root) is the primary control and would still hold, so this
/// is defence in depth -- but domain separation is the stated contract, so it gets a test that
/// actually bites.
#[test]
fn the_two_sync_purposes_are_distinct_domains() {
    assert_ne!(
        FLEET_SYNC_ENVELOPE_PURPOSE, VESSEL_SYNC_ENVELOPE_PURPOSE,
        "Fleet->Vessel and Vessel->Fleet sync envelopes must occupy different signing domains"
    );

    let (root, _) = root();
    let bound = binding("device-domain-sep");
    let (authority, device) = enrolled(&bound, [23; 32]);

    let payload = BoundSyncEnvelopeV1 {
        binding: bound.clone(),
        record: sync_record(&bound, "sync-record-domain"),
    };

    // A device signing under the FLEET direction's purpose must not be admitted as a Vessel->Fleet
    // sync envelope, even though the payload bytes and the signing key are otherwise valid.
    let signed_for_fleet_direction = device.sign(FLEET_SYNC_ENVELOPE_PURPOSE, payload).unwrap();

    assert_eq!(
        signed_for_fleet_direction.verify(&device.public(), VESSEL_SYNC_ENVELOPE_PURPOSE),
        Err(EnrollmentError::Verification),
        "a signature made under the Fleet sync purpose must not verify under the Vessel sync purpose"
    );
    assert_eq!(
        authority.admit_vessel_sync(&signed_for_fleet_direction, &root, 12),
        Err(EnrollmentError::Verification),
        "admission must reject an envelope signed for the opposite sync direction"
    );
}
