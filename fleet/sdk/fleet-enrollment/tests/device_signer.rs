use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use fleet_enrollment::{
    DeviceSignerV1, EnrollmentChallengeV1, EnrollmentError, IdentityBindingV1,
    VesselDeviceKeypairV1, respond_to_enrollment_v1, sign_device_envelope_v1,
};
use fleet_protocol::{DigestValue, FleetId, FleetNodeId, KeyId, TenantId, VesselId, digest};
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn binding(key: &str) -> IdentityBindingV1 {
    IdentityBindingV1 {
        tenant_id: TenantId::new("tenant").unwrap(),
        fleet_id: FleetId::new("fleet").unwrap(),
        vessel_id: VesselId::new("vessel").unwrap(),
        fleet_node_id: FleetNodeId::new("node").unwrap(),
        key_id: KeyId::new(key).unwrap(),
    }
}

struct TraitOnly(Arc<VesselDeviceKeypairV1>);
impl DeviceSignerV1 for TraitOnly {
    fn public_key(&self) -> Result<fleet_enrollment::DevicePublicKeyV1, EnrollmentError> {
        DeviceSignerV1::public_key(self.0.as_ref())
    }
    fn sign_digest(&self, digest: &DigestValue) -> Result<[u8; 64], EnrollmentError> {
        DeviceSignerV1::sign_digest(self.0.as_ref(), digest)
    }
}

#[test]
fn payload_and_challenge_match_independent_v1_baselines() {
    let device = VesselDeviceKeypairV1::from_private_bytes(KeyId::new("k1").unwrap(), [7; 32]);
    let signer: &dyn DeviceSignerV1 = &TraitOnly(Arc::new(device));
    let payload = serde_json::json!({"message":"hello"});
    let envelope = sign_device_envelope_v1(signer, "payload-v1", payload.clone()).unwrap();
    let digest = digest("payload-v1", &serde_json::to_vec(&payload).unwrap()).unwrap();
    let key = SigningKey::from_bytes(&[7; 32]);
    assert_eq!(envelope.payload_digest, digest);
    assert_eq!(
        envelope.signature.as_slice(),
        key.sign(digest.as_str().as_bytes()).to_bytes()
    );
    VerifyingKey::from(&key)
        .verify(
            digest.as_str().as_bytes(),
            &Signature::from_bytes(&envelope.signature.as_slice().try_into().unwrap()),
        )
        .unwrap();

    let challenge = EnrollmentChallengeV1::new(
        binding("k1"),
        signer.public_key().unwrap(),
        vec![3; 32],
        10,
        20,
    )
    .unwrap();
    let response = respond_to_enrollment_v1(signer, challenge.clone()).unwrap();
    let challenge_digest = challenge.digest().unwrap();
    assert_eq!(
        response.device_signature.as_slice(),
        key.sign(challenge_digest.as_str().as_bytes()).to_bytes()
    );
    VerifyingKey::from(&key)
        .verify(
            challenge_digest.as_str().as_bytes(),
            &Signature::from_bytes(&response.device_signature.as_slice().try_into().unwrap()),
        )
        .unwrap();
}

#[test]
fn wrong_purpose_tamper_and_key_mismatch_are_rejected() {
    let device = VesselDeviceKeypairV1::from_private_bytes(KeyId::new("k1").unwrap(), [8; 32]);
    let envelope = device.sign("purpose-a", "payload").unwrap();
    assert_eq!(
        envelope.verify(&device.public(), "purpose-b"),
        Err(EnrollmentError::Verification)
    );
    let mut tampered = envelope.clone();
    tampered.payload = "changed";
    assert_eq!(
        tampered.verify(&device.public(), "purpose-a"),
        Err(EnrollmentError::Verification)
    );
    let other = VesselDeviceKeypairV1::from_private_bytes(KeyId::new("other").unwrap(), [9; 32]);
    let challenge =
        EnrollmentChallengeV1::new(binding("other"), other.public(), vec![1; 32], 1, 2).unwrap();
    assert_eq!(
        respond_to_enrollment_v1(&device, challenge),
        Err(EnrollmentError::KeyDenied)
    );
}

struct Failing {
    calls: Arc<AtomicUsize>,
}
impl DeviceSignerV1 for Failing {
    fn public_key(&self) -> Result<fleet_enrollment::DevicePublicKeyV1, EnrollmentError> {
        Err(EnrollmentError::Malformed)
    }
    fn sign_digest(&self, _: &DigestValue) -> Result<[u8; 64], EnrollmentError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(EnrollmentError::KeyDenied)
    }
}

#[derive(Serialize)]
struct Huge(String);
#[test]
fn provider_public_key_errors_propagate_before_signing() {
    let device = VesselDeviceKeypairV1::from_private_bytes(KeyId::new("k1").unwrap(), [8; 32]);
    let challenge =
        EnrollmentChallengeV1::new(binding("k1"), device.public(), vec![1; 32], 1, 2).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let signer = Failing {
        calls: calls.clone(),
    };
    assert_eq!(
        respond_to_enrollment_v1(&signer, challenge),
        Err(EnrollmentError::Malformed)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn provider_errors_propagate_and_oversize_precedes_signing() {
    let calls = Arc::new(AtomicUsize::new(0));
    let signer = Failing {
        calls: calls.clone(),
    };
    assert_eq!(
        sign_device_envelope_v1(&signer, "x", "ok"),
        Err(EnrollmentError::KeyDenied)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        sign_device_envelope_v1(&signer, "x", Huge("x".repeat(64 * 1024))),
        Err(EnrollmentError::Malformed)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
