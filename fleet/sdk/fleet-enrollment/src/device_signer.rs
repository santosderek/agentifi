use super::{
    BoundSyncEnvelopeV1, DevicePublicKeyV1, DeviceSignedEnvelopeV1, EnrollmentChallengeV1,
    EnrollmentError, EnrollmentResponseV1, VESSEL_SYNC_ENVELOPE_PURPOSE, VesselDeviceKeypairV1,
    signable,
};
use ed25519_dalek::Signer as _;
use fleet_protocol::DigestValue;
use serde::Serialize;

/// V1 Ed25519 device signing port; implementations retain private-key custody.
///
/// `sign_digest` signs exactly `digest.as_str().as_bytes()` and returns 64 bytes.
/// Provider errors propagate unchanged; this interface makes no hardware-custody claim.
pub trait DeviceSignerV1: Send + Sync {
    fn public_key(&self) -> Result<DevicePublicKeyV1, EnrollmentError>;
    fn sign_digest(&self, digest: &DigestValue) -> Result<[u8; 64], EnrollmentError>;
}

/// Build and sign a bounded V1 envelope using the canonical digest representation.
/// The provider signs the exact ASCII `digest.as_str().as_bytes()` message and returns 64 bytes;
/// provider errors propagate, with no hardware-custody guarantee.
pub fn sign_device_envelope_v1<T: Serialize>(
    signer: &dyn DeviceSignerV1,
    purpose: &str,
    payload: T,
) -> Result<DeviceSignedEnvelopeV1<T>, EnrollmentError> {
    let payload_digest = signable(purpose, &payload)?;
    let signature = signer.sign_digest(&payload_digest)?;
    Ok(DeviceSignedEnvelopeV1 {
        payload,
        payload_digest,
        signature: signature.to_vec(),
    })
}

/// Build a Vessel -> Fleet synchronisation envelope signed by the device key.
///
/// This is the outbound builder an edge device uses instead of the Fleet-root builder, so no Fleet
/// root signing key is ever required on a Vessel. The purpose is pinned to
/// [`VESSEL_SYNC_ENVELOPE_PURPOSE`] so the result cannot be presented as Fleet-signed material.
///
/// The caller must supply a binding that matches the key actually held by `signer`; the identity is
/// covered by the signature, so a mismatch is rejected by
/// [`crate::FleetEnrollmentAuthorityV1::admit_vessel_sync`] rather than silently accepted.
pub fn build_device_signed_sync_envelope_v1(
    signer: &dyn DeviceSignerV1,
    envelope: BoundSyncEnvelopeV1,
) -> Result<DeviceSignedEnvelopeV1<BoundSyncEnvelopeV1>, EnrollmentError> {
    let public = signer.public_key()?;
    if envelope.binding.key_id != public.key_id {
        return Err(EnrollmentError::KeyDenied);
    }
    envelope.record.validate()?;
    sign_device_envelope_v1(signer, VESSEL_SYNC_ENVELOPE_PURPOSE, envelope)
}

/// Sign a validated V1 enrollment challenge with the bound Ed25519 key.
/// The provider signs `digest.as_str().as_bytes()` and returns 64 bytes; errors propagate and
/// ordinary providers do not imply hardware custody.
pub fn respond_to_enrollment_v1(
    signer: &dyn DeviceSignerV1,
    challenge: EnrollmentChallengeV1,
) -> Result<EnrollmentResponseV1, EnrollmentError> {
    let public = signer.public_key()?;
    if challenge.binding.key_id != public.key_id {
        return Err(EnrollmentError::KeyDenied);
    }
    challenge.validate()?;
    let challenge_digest = challenge.digest()?;
    let signature = signer.sign_digest(&challenge_digest)?;
    Ok(EnrollmentResponseV1 {
        challenge,
        device_signature: signature.to_vec(),
    })
}

impl DeviceSignerV1 for VesselDeviceKeypairV1 {
    fn public_key(&self) -> Result<DevicePublicKeyV1, EnrollmentError> {
        Ok(self.public())
    }

    fn sign_digest(&self, digest: &DigestValue) -> Result<[u8; 64], EnrollmentError> {
        Ok(self.signing_key.sign(digest.as_str().as_bytes()).to_bytes())
    }
}
