//! Transport-neutral Fleet enrollment authority.
//!
//! This crate deliberately contains no HTTP, NATS, SQL, or filesystem access.  Its callers own
//! private-key custody and durable replay protection; the types here make every cross-boundary
//! decision bind tenant, Fleet, Vessel, Fleet node, and key identity.

use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use fleet_protocol::{
    A2ATaskActionV1, A2ATaskV1, AgentCardId, AgentCardV1, DigestValue, FleetId, FleetNodeId,
    FleetRecordPayloadV1, FleetRecordV1, KeyId, PolicyEnvelopeV1, ProtocolError,
    RegistrationStateV1, TaskId, TenantId, VesselId, digest,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use zeroize::Zeroize as _;

mod device_signer;
pub use device_signer::{
    DeviceSignerV1, build_device_signed_sync_envelope_v1, respond_to_enrollment_v1,
    sign_device_envelope_v1,
};

pub const MAX_ENROLLMENT_BYTES: usize = 64 * 1024;

/// Signing purpose for the Fleet -> Vessel synchronisation direction.
///
/// Envelopes carrying this purpose are signed by the Fleet **root** authority and verified against
/// [`FleetTrustRootV1`]. See [`FleetEnrollmentAuthorityV1::admit_sync`].
pub const FLEET_SYNC_ENVELOPE_PURPOSE: &str = "fleet-sync-envelope";

/// Signing purpose for the Vessel -> Fleet synchronisation direction.
///
/// Envelopes carrying this purpose are signed by the **device** key whose public half Fleet already
/// recorded at enrollment, and are verified against that [`DevicePublicKeyV1`]. See
/// [`FleetEnrollmentAuthorityV1::admit_vessel_sync`].
///
/// The two purposes are deliberately distinct strings because the purpose is mixed into the payload
/// digest that is actually signed. A device-signed envelope therefore cannot be replayed as a
/// Fleet-signed one (which would be a privilege escalation, since Fleet-signed material is the
/// authority for grants and policy), and a Fleet-signed envelope cannot be replayed as if a device
/// had attested its own local execution facts.
pub const VESSEL_SYNC_ENVELOPE_PURPOSE: &str = "vessel-sync-envelope";

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum EnrollmentError {
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("malformed enrollment material")]
    Malformed,
    #[error("signature verification failed")]
    Verification,
    #[error("challenge expired, replayed, or not issued")]
    ChallengeDenied,
    #[error("Fleet/Vessel/key binding is not admitted")]
    AdmissionDenied,
    #[error("key is revoked or stale")]
    KeyDenied,
    #[error("envelope is not a Fleet-signed connected envelope")]
    UnsignedFixtureDenied,
}
impl From<ProtocolError> for EnrollmentError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value.to_string())
    }
}

fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, EnrollmentError> {
    let bytes = serde_json::to_vec(value).map_err(|_| EnrollmentError::Malformed)?;
    if bytes.len() > MAX_ENROLLMENT_BYTES {
        return Err(EnrollmentError::Malformed);
    }
    Ok(bytes)
}
fn signable<T: Serialize>(purpose: &str, value: &T) -> Result<DigestValue, EnrollmentError> {
    Ok(digest(purpose, &encoded(value)?)?)
}
fn public_key(bytes: &[u8]) -> Result<VerifyingKey, EnrollmentError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| EnrollmentError::Malformed)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| EnrollmentError::Malformed)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct IdentityBindingV1 {
    pub tenant_id: TenantId,
    pub fleet_id: FleetId,
    pub vessel_id: VesselId,
    pub fleet_node_id: FleetNodeId,
    pub key_id: KeyId,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FleetTrustRootV1 {
    pub tenant_id: TenantId,
    pub fleet_id: FleetId,
    pub key_id: KeyId,
    pub ed25519_public_key: Vec<u8>,
}
impl FleetTrustRootV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        public_key(&self.ed25519_public_key).map(|_| ())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DevicePublicKeyV1 {
    pub key_id: KeyId,
    pub ed25519_public_key: Vec<u8>,
}
impl DevicePublicKeyV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        public_key(&self.ed25519_public_key).map(|_| ())
    }
}

/// Private device key material. It is intentionally not serializable and must only be put in
/// private Vessel storage by a local adapter.
#[derive(Debug)]
pub struct VesselDeviceKeypairV1 {
    key_id: KeyId,
    signing_key: SigningKey,
}
impl VesselDeviceKeypairV1 {
    pub fn generate(key_id: KeyId) -> Result<Self, EnrollmentError> {
        let mut seed = [0_u8; 32];
        getrandom::fill(&mut seed).map_err(|_| EnrollmentError::Malformed)?;
        Ok(Self {
            key_id,
            signing_key: SigningKey::from_bytes(&seed),
        })
    }
    pub fn from_private_bytes(key_id: KeyId, bytes: [u8; 32]) -> Self {
        Self {
            key_id,
            signing_key: SigningKey::from_bytes(&bytes),
        }
    }
    pub fn public(&self) -> DevicePublicKeyV1 {
        DevicePublicKeyV1 {
            key_id: self.key_id.clone(),
            ed25519_public_key: self.signing_key.verifying_key().to_bytes().to_vec(),
        }
    }
    pub fn respond(
        &self,
        challenge: EnrollmentChallengeV1,
    ) -> Result<EnrollmentResponseV1, EnrollmentError> {
        respond_to_enrollment_v1(self, challenge)
    }

    /// Signs a Vessel -> Fleet synchronisation envelope with this device key.
    pub fn sign_sync(
        &self,
        envelope: BoundSyncEnvelopeV1,
    ) -> Result<DeviceSignedEnvelopeV1<BoundSyncEnvelopeV1>, EnrollmentError> {
        build_device_signed_sync_envelope_v1(self, envelope)
    }

    /// Signs an exact, bounded transport-neutral request. The purpose is part of the signature.
    pub fn sign<T: Serialize>(
        &self,
        purpose: &str,
        payload: T,
    ) -> Result<DeviceSignedEnvelopeV1<T>, EnrollmentError> {
        sign_device_envelope_v1(self, purpose, payload)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentChallengeV1 {
    pub binding: IdentityBindingV1,
    pub public_key: DevicePublicKeyV1,
    pub nonce: Vec<u8>,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}
impl EnrollmentChallengeV1 {
    pub fn new(
        binding: IdentityBindingV1,
        public_key: DevicePublicKeyV1,
        nonce: Vec<u8>,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self, EnrollmentError> {
        let value = Self {
            binding,
            public_key,
            nonce,
            issued_at_ms,
            expires_at_ms,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        if self.binding.key_id != self.public_key.key_id
            || self.nonce.len() != 32
            || self.issued_at_ms >= self.expires_at_ms
        {
            return Err(EnrollmentError::Malformed);
        }
        self.public_key.validate()
    }
    pub fn digest(&self) -> Result<DigestValue, EnrollmentError> {
        self.validate()?;
        signable("fleet-enrollment-challenge", self)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentResponseV1 {
    pub challenge: EnrollmentChallengeV1,
    pub device_signature: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VesselKeyStateV1 {
    Active,
    Rotated,
    Revoked,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VesselKeyRecordV1 {
    pub binding: IdentityBindingV1,
    pub public_key: DevicePublicKeyV1,
    pub state: VesselKeyStateV1,
    pub revision: u64,
    pub expires_at_ms: u64,
}
impl VesselKeyRecordV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        if self.binding.key_id != self.public_key.key_id || self.revision == 0 {
            return Err(EnrollmentError::Malformed);
        }
        self.public_key.validate()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentGrantV1 {
    pub key: VesselKeyRecordV1,
    pub registration: RegistrationStateV1,
    pub issued_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FleetSignatureV1 {
    pub key_id: KeyId,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FleetSignedEnvelopeV1<T> {
    pub payload: T,
    pub payload_digest: DigestValue,
    pub signature: FleetSignatureV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeviceSignedEnvelopeV1<T> {
    pub payload: T,
    pub payload_digest: DigestValue,
    pub signature: Vec<u8>,
}
impl<T: Serialize> DeviceSignedEnvelopeV1<T> {
    pub fn verify(&self, public: &DevicePublicKeyV1, purpose: &str) -> Result<(), EnrollmentError> {
        public.validate()?;
        if self.payload_digest != signable(purpose, &self.payload)? {
            return Err(EnrollmentError::Verification);
        }
        let signature: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| EnrollmentError::Malformed)?;
        public_key(&public.ed25519_public_key)?
            .verify(
                self.payload_digest.as_str().as_bytes(),
                &Signature::from_bytes(&signature),
            )
            .map_err(|_| EnrollmentError::Verification)
    }
}

#[derive(Debug)]
pub struct FleetAuthoritySigner {
    root: FleetTrustRootV1,
    key: SigningKey,
}
impl FleetAuthoritySigner {
    pub fn from_seed(root: FleetTrustRootV1, mut seed: [u8; 32]) -> Result<Self, EnrollmentError> {
        let key = SigningKey::from_bytes(&seed);
        seed.zeroize();
        root.validate()?;
        if key.verifying_key().to_bytes().as_slice() != root.ed25519_public_key.as_slice() {
            return Err(EnrollmentError::KeyDenied);
        }
        Ok(Self { root, key })
    }
    pub fn trust_root(&self) -> &FleetTrustRootV1 {
        &self.root
    }
    pub fn sign<T: Serialize>(
        &self,
        purpose: &str,
        payload: T,
    ) -> Result<FleetSignedEnvelopeV1<T>, EnrollmentError> {
        let payload_digest = signable(purpose, &payload)?;
        let signature = self
            .key
            .sign(payload_digest.as_str().as_bytes())
            .to_bytes()
            .to_vec();
        Ok(FleetSignedEnvelopeV1 {
            payload,
            payload_digest,
            signature: FleetSignatureV1 {
                key_id: self.root.key_id.clone(),
                signature,
            },
        })
    }
}
impl<T: Serialize> FleetSignedEnvelopeV1<T> {
    pub fn verify(&self, root: &FleetTrustRootV1, purpose: &str) -> Result<(), EnrollmentError> {
        root.validate()?;
        if self.signature.key_id != root.key_id
            || self.payload_digest != signable(purpose, &self.payload)?
        {
            return Err(EnrollmentError::Verification);
        }
        let signature: [u8; 64] = self
            .signature
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| EnrollmentError::Malformed)?;
        public_key(&root.ed25519_public_key)?
            .verify(
                self.payload_digest.as_str().as_bytes(),
                &Signature::from_bytes(&signature),
            )
            .map_err(|_| EnrollmentError::Verification)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundPolicyEnvelopeV1 {
    pub binding: IdentityBindingV1,
    pub policy: PolicyEnvelopeV1,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundSyncEnvelopeV1 {
    pub binding: IdentityBindingV1,
    pub record: FleetRecordV1,
}

/// Fleet-root-attested card provenance. A raw [`AgentCardV1`] is descriptive only.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FleetCardProvenanceV1 {
    pub tenant_id: TenantId,
    pub fleet_id: FleetId,
    pub issuer_node_id: FleetNodeId,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundAgentCardV1 {
    pub binding: IdentityBindingV1,
    pub card: AgentCardV1,
    pub provenance: FleetCardProvenanceV1,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

/// Fleet-root-issued, one-use authority to enroll exactly one device public key for one scope.
/// It is deliberately required for both initial enrollment and replacement-key recovery.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentAuthorizationV1 {
    pub binding: IdentityBindingV1,
    pub public_key: DevicePublicKeyV1,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}
impl EnrollmentAuthorizationV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        if self.binding.key_id != self.public_key.key_id
            || self.issued_at_ms >= self.expires_at_ms
            || self.expires_at_ms - self.issued_at_ms > 24 * 60 * 60 * 1000
        {
            return Err(EnrollmentError::Malformed);
        }
        self.public_key.validate()
    }
}

/// Device-authenticated request for Fleet Core to issue a card for the active exact key.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CardRegistrationV1 {
    pub binding: IdentityBindingV1,
    pub card: AgentCardV1,
    pub policy_envelope_digest: DigestValue,
}
impl CardRegistrationV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        self.card.validate()?;
        if self.binding.vessel_id != self.card.vessel_id
            || self.binding.fleet_node_id != self.card.fleet_node_id
            || self.binding.key_id != self.card.key_id
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct A2aTaskSubmissionV1 {
    pub binding: IdentityBindingV1,
    pub card: FleetSignedEnvelopeV1<BoundAgentCardV1>,
    pub policy: FleetSignedEnvelopeV1<BoundPolicyEnvelopeV1>,
    pub registry_revision: u64,
    pub registry_digest: DigestValue,
    pub task: A2ATaskV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct A2aTaskCommandV1 {
    pub binding: IdentityBindingV1,
    pub card_id: AgentCardId,
    pub card_revision: u64,
    pub task_id: TaskId,
    pub operation: A2aTaskOperationV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum A2aTaskOperationV1 {
    Status,
    Cancel { reason: String },
    HumanInput { response: String },
}
impl A2aTaskOperationV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        match self {
            Self::Status => Ok(()),
            Self::Cancel { reason } if !reason.is_empty() && reason.len() <= 1024 => Ok(()),
            Self::HumanInput { response }
                if !response.is_empty() && response.len() <= fleet_protocol::MAX_TEXT_BYTES =>
            {
                Ok(())
            }
            _ => Err(EnrollmentError::Malformed),
        }
    }
    pub fn as_action(&self) -> Option<A2ATaskActionV1> {
        match self {
            Self::Status => None,
            Self::Cancel { reason } => Some(A2ATaskActionV1::Cancel {
                reason: reason.clone(),
            }),
            Self::HumanInput { response } => Some(A2ATaskActionV1::HumanInput {
                prompt: "fleet-core-request".into(),
                response: Some(response.clone()),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct A2aCapabilityRegistryV1 {
    revision: u64,
    capabilities: BTreeSet<String>,
    digest: DigestValue,
}
impl A2aCapabilityRegistryV1 {
    pub fn new(revision: u64, capabilities: BTreeSet<String>) -> Result<Self, EnrollmentError> {
        if revision == 0 || capabilities.is_empty() || capabilities.len() > 256 {
            return Err(EnrollmentError::Malformed);
        }
        if capabilities.iter().any(|capability| {
            capability.is_empty()
                || capability.len() > 128
                || !capability.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                })
        }) {
            return Err(EnrollmentError::Malformed);
        }
        let digest = signable("fleet-a2a-capability-registry", &(revision, &capabilities))?;
        Ok(Self {
            revision,
            capabilities,
            digest,
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn digest(&self) -> &DigestValue {
        &self.digest
    }
    pub fn contains(&self, capability: &str) -> bool {
        self.capabilities.contains(capability)
    }
}

/// In-memory authority projection. SQL adapters persist the exact challenge, grant, and key
/// rows. This projection records issued challenges so callers cannot manufacture one locally.
#[derive(Clone, Debug, Default)]
pub struct FleetEnrollmentAuthorityV1 {
    issued: BTreeMap<DigestValue, EnrollmentChallengeV1>,
    consumed: BTreeSet<DigestValue>,
    keys: BTreeMap<IdentityBindingV1, VesselKeyRecordV1>,
    registration: BTreeMap<(TenantId, FleetId, VesselId, FleetNodeId), RegistrationStateV1>,
}
impl FleetEnrollmentAuthorityV1 {
    fn scope(binding: &IdentityBindingV1) -> (TenantId, FleetId, VesselId, FleetNodeId) {
        (
            binding.tenant_id.clone(),
            binding.fleet_id.clone(),
            binding.vessel_id.clone(),
            binding.fleet_node_id.clone(),
        )
    }

    pub fn issue_challenge(
        &mut self,
        binding: IdentityBindingV1,
        public_key: DevicePublicKeyV1,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<EnrollmentChallengeV1, EnrollmentError> {
        let mut nonce = vec![0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| EnrollmentError::Malformed)?;
        let challenge = EnrollmentChallengeV1::new(
            binding,
            public_key,
            nonce,
            now_ms,
            now_ms
                .checked_add(ttl_ms)
                .ok_or(EnrollmentError::Malformed)?,
        )?;
        let digest = challenge.digest()?;
        if self.issued.insert(digest, challenge.clone()).is_some() {
            return Err(EnrollmentError::ChallengeDenied);
        }
        Ok(challenge)
    }

    pub fn accept_response(
        &mut self,
        response: EnrollmentResponseV1,
        now_ms: u64,
        key_revision: u64,
        expires_at_ms: u64,
    ) -> Result<EnrollmentGrantV1, EnrollmentError> {
        response.challenge.validate()?;
        let challenge_digest = response.challenge.digest()?;
        let issued = self
            .issued
            .get(&challenge_digest)
            .ok_or(EnrollmentError::ChallengeDenied)?;
        if issued != &response.challenge
            || now_ms < issued.issued_at_ms
            || now_ms >= issued.expires_at_ms
            || self.consumed.contains(&challenge_digest)
            || key_revision == 0
            || now_ms >= expires_at_ms
        {
            return Err(EnrollmentError::ChallengeDenied);
        }
        let signature: [u8; 64] = response
            .device_signature
            .as_slice()
            .try_into()
            .map_err(|_| EnrollmentError::Malformed)?;
        public_key(&response.challenge.public_key.ed25519_public_key)?
            .verify(
                challenge_digest.as_str().as_bytes(),
                &Signature::from_bytes(&signature),
            )
            .map_err(|_| EnrollmentError::Verification)?;

        let key = VesselKeyRecordV1 {
            binding: response.challenge.binding.clone(),
            public_key: response.challenge.public_key,
            state: VesselKeyStateV1::Active,
            revision: key_revision,
            expires_at_ms,
        };
        key.validate()?;
        let scope = Self::scope(&key.binding);
        if self.registration.get(&scope) == Some(&RegistrationStateV1::Revoked)
            || self.keys.get(&key.binding).is_some_and(|existing| {
                existing.state == VesselKeyStateV1::Revoked || existing.revision >= key.revision
            })
        {
            return Err(EnrollmentError::KeyDenied);
        }
        let previous = self
            .keys
            .iter()
            .find(|(binding, existing)| {
                Self::scope(binding) == scope && existing.state == VesselKeyStateV1::Active
            })
            .map(|(binding, existing)| (binding.clone(), existing.revision));
        if let Some((previous_binding, previous_revision)) = &previous
            && (*previous_binding == key.binding || key.revision <= *previous_revision)
        {
            return Err(EnrollmentError::KeyDenied);
        }

        // All rejection checks, including signature verification, happen before this state change.
        self.consumed.insert(challenge_digest);
        if let Some((previous_binding, _)) = previous
            && let Some(previous_key) = self.keys.get_mut(&previous_binding)
        {
            previous_key.state = VesselKeyStateV1::Rotated;
        }
        self.keys.insert(key.binding.clone(), key.clone());
        self.registration.insert(scope, RegistrationStateV1::Active);
        Ok(EnrollmentGrantV1 {
            key,
            registration: RegistrationStateV1::Active,
            issued_at_ms: now_ms,
        })
    }

    /// Rotate only with a fresh, Fleet-issued challenge signed by the replacement device key.
    pub fn rotate(
        &mut self,
        old: &IdentityBindingV1,
        response: EnrollmentResponseV1,
        now_ms: u64,
        key_revision: u64,
        expires_at_ms: u64,
    ) -> Result<EnrollmentGrantV1, EnrollmentError> {
        let old_key = self.keys.get(old).ok_or(EnrollmentError::AdmissionDenied)?;
        if old_key.state != VesselKeyStateV1::Active
            || Self::scope(old) != Self::scope(&response.challenge.binding)
            || response.challenge.binding.key_id == old.key_id
            || key_revision <= old_key.revision
        {
            return Err(EnrollmentError::KeyDenied);
        }
        self.accept_response(response, now_ms, key_revision, expires_at_ms)
    }

    pub fn revoke(&mut self, binding: &IdentityBindingV1) -> Result<(), EnrollmentError> {
        let key = self
            .keys
            .get_mut(binding)
            .ok_or(EnrollmentError::AdmissionDenied)?;
        let was_active = key.state == VesselKeyStateV1::Active;
        key.state = VesselKeyStateV1::Revoked;
        if was_active {
            self.registration
                .insert(Self::scope(binding), RegistrationStateV1::Revoked);
        }
        Ok(())
    }

    pub fn reconstruct(
        issued: Vec<(EnrollmentChallengeV1, bool)>,
        keys: Vec<VesselKeyRecordV1>,
        registrations: Vec<(IdentityBindingV1, RegistrationStateV1)>,
    ) -> Result<Self, EnrollmentError> {
        let mut authority = Self::default();
        for (challenge, consumed) in issued {
            challenge.validate()?;
            let challenge_digest = challenge.digest()?;
            if authority
                .issued
                .insert(challenge_digest.clone(), challenge)
                .is_some()
            {
                return Err(EnrollmentError::Malformed);
            }
            if consumed {
                authority.consumed.insert(challenge_digest);
            }
        }
        for key in keys {
            key.validate()?;
            if authority.keys.insert(key.binding.clone(), key).is_some() {
                return Err(EnrollmentError::Malformed);
            }
        }
        for (binding, state) in registrations {
            let scope = Self::scope(&binding);
            if authority.registration.insert(scope, state).is_some() {
                return Err(EnrollmentError::Malformed);
            }
        }
        for (scope, state) in &authority.registration {
            let active = authority
                .keys
                .values()
                .filter(|key| {
                    Self::scope(&key.binding) == *scope && key.state == VesselKeyStateV1::Active
                })
                .count();
            if (*state == RegistrationStateV1::Active && active != 1)
                || (*state == RegistrationStateV1::Revoked && active != 0)
            {
                return Err(EnrollmentError::Malformed);
            }
        }
        Ok(authority)
    }

    pub fn active_key(
        &self,
        binding: &IdentityBindingV1,
        now_ms: u64,
    ) -> Result<&VesselKeyRecordV1, EnrollmentError> {
        if !self.active(binding, now_ms) {
            return Err(EnrollmentError::AdmissionDenied);
        }
        self.keys
            .get(binding)
            .ok_or(EnrollmentError::AdmissionDenied)
    }

    fn active(&self, binding: &IdentityBindingV1, now_ms: u64) -> bool {
        self.registration.get(&Self::scope(binding)) == Some(&RegistrationStateV1::Active)
            && self.keys.get(binding).is_some_and(|key| {
                key.state == VesselKeyStateV1::Active && now_ms < key.expires_at_ms
            })
    }

    pub fn admit_enrollment_authorization(
        &self,
        signed: &FleetSignedEnvelopeV1<EnrollmentAuthorizationV1>,
        root: &FleetTrustRootV1,
        binding: &IdentityBindingV1,
        public: &DevicePublicKeyV1,
        now_ms: u64,
    ) -> Result<(), EnrollmentError> {
        signed.verify(root, "fleet-enrollment-authorization")?;
        signed.payload.validate()?;
        if signed.payload.binding != *binding
            || signed.payload.public_key != *public
            || binding.tenant_id != root.tenant_id
            || binding.fleet_id != root.fleet_id
            || now_ms < signed.payload.issued_at_ms
            || now_ms >= signed.payload.expires_at_ms
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        Ok(())
    }

    pub fn admit_card_registration(
        &self,
        signed: &DeviceSignedEnvelopeV1<CardRegistrationV1>,
        policy: &FleetSignedEnvelopeV1<BoundPolicyEnvelopeV1>,
        root: &FleetTrustRootV1,
        now_ms: u64,
    ) -> Result<(), EnrollmentError> {
        signed.payload.validate()?;
        let binding = &signed.payload.binding;
        let key = self.active_key(binding, now_ms)?;
        signed.verify(&key.public_key, "fleet-agent-card-registration")?;
        self.admit_policy(policy, root, now_ms)?;
        if policy.payload.binding != *binding
            || signed.payload.policy_envelope_digest != policy.payload_digest
            || signed.payload.card.policy_digest != policy.payload.policy.policy_digest
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        Ok(())
    }

    pub fn admit_policy(
        &self,
        signed: &FleetSignedEnvelopeV1<BoundPolicyEnvelopeV1>,
        root: &FleetTrustRootV1,
        now_ms: u64,
    ) -> Result<(), EnrollmentError> {
        signed.verify(root, "fleet-policy-envelope")?;
        let value = &signed.payload;
        value.policy.validate()?;
        if !self.active(&value.binding, now_ms)
            || value.binding.tenant_id != root.tenant_id
            || value.binding.fleet_id != root.fleet_id
            || value.binding.vessel_id != value.policy.vessel_id
            || now_ms < value.policy.not_before_ms
            || now_ms >= value.policy.expires_at_ms
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        Ok(())
    }

    pub fn admit_sync(
        &self,
        signed: &FleetSignedEnvelopeV1<BoundSyncEnvelopeV1>,
        root: &FleetTrustRootV1,
        now_ms: u64,
    ) -> Result<(), EnrollmentError> {
        signed.verify(root, FLEET_SYNC_ENVELOPE_PURPOSE)?;
        let value = &signed.payload;
        value.record.validate()?;
        let header = &value.record.header;
        if !self.active(&value.binding, now_ms)
            || value.binding.tenant_id != root.tenant_id
            || value.binding.fleet_id != root.fleet_id
            || header.tenant_id != value.binding.tenant_id
            || header.fleet_id != value.binding.fleet_id
            || header.vessel_id.as_ref() != Some(&value.binding.vessel_id)
            || header.provenance.origin_node_id != value.binding.fleet_node_id
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        match &value.record.payload {
            FleetRecordPayloadV1::SyncManifest(manifest)
                if manifest.vessel_id == value.binding.vessel_id =>
            {
                Ok(())
            }
            FleetRecordPayloadV1::SyncBatch(_) | FleetRecordPayloadV1::SyncCheckpoint(_) => Ok(()),
            FleetRecordPayloadV1::SyncManifest(_) => Err(EnrollmentError::AdmissionDenied),
            _ => Err(EnrollmentError::UnsignedFixtureDenied),
        }
    }

    /// Admits a Vessel -> Fleet synchronisation envelope signed by the **device** key.
    ///
    /// This is the inbound counterpart of [`Self::admit_sync`]. It exists so an edge device never
    /// needs the Fleet root signing key: Fleet already holds the device's *public* key from
    /// enrollment, which is sufficient to verify the device's own signature. A device private key
    /// is never transmitted and never stored Fleet-side.
    ///
    /// Verification order is deliberate. The key is resolved from the binding carried **inside the
    /// signed payload**, so the signature is only ever checked against the key that the claimed
    /// identity actually enrolled. Because the binding is covered by the signature, an attacker
    /// cannot relabel a valid envelope from device A as device B: editing the binding invalidates
    /// the digest, and leaving it intact still resolves to device A's key.
    ///
    /// `active_key` enforces the key lifecycle for free: a registration that is not `Active`, a key
    /// in `Rotated` or `Revoked` state, or a grant whose `expires_at_ms` has passed all fail here
    /// before any signature work is attempted.
    pub fn admit_vessel_sync(
        &self,
        signed: &DeviceSignedEnvelopeV1<BoundSyncEnvelopeV1>,
        root: &FleetTrustRootV1,
        now_ms: u64,
    ) -> Result<(), EnrollmentError> {
        root.validate()?;
        let value = &signed.payload;
        value.record.validate()?;
        // Resolve the enrolled key for the signed binding first; this rejects revoked, rotated,
        // expired, and unregistered identities before the signature is considered.
        let key = self.active_key(&value.binding, now_ms)?;
        signed.verify(&key.public_key, VESSEL_SYNC_ENVELOPE_PURPOSE)?;
        let header = &value.record.header;
        if value.binding.tenant_id != root.tenant_id
            || value.binding.fleet_id != root.fleet_id
            || key.public_key.key_id != value.binding.key_id
            || header.tenant_id != value.binding.tenant_id
            || header.fleet_id != value.binding.fleet_id
            || header.vessel_id.as_ref() != Some(&value.binding.vessel_id)
            || header.provenance.origin_node_id != value.binding.fleet_node_id
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        match &value.record.payload {
            FleetRecordPayloadV1::SyncManifest(manifest)
                if manifest.vessel_id == value.binding.vessel_id =>
            {
                Ok(())
            }
            FleetRecordPayloadV1::SyncBatch(_) | FleetRecordPayloadV1::SyncCheckpoint(_) => Ok(()),
            FleetRecordPayloadV1::SyncManifest(_) => Err(EnrollmentError::AdmissionDenied),
            _ => Err(EnrollmentError::UnsignedFixtureDenied),
        }
    }

    pub fn admit_signed_card(
        &self,
        card: &FleetSignedEnvelopeV1<BoundAgentCardV1>,
        policy: &FleetSignedEnvelopeV1<BoundPolicyEnvelopeV1>,
        root: &FleetTrustRootV1,
        registry: &A2aCapabilityRegistryV1,
        now_ms: u64,
    ) -> Result<(), EnrollmentError> {
        card.verify(root, "fleet-agent-card")?;
        policy.verify(root, "fleet-policy-envelope")?;
        let value = &card.payload;
        self.admit_card(&value.card, &value.binding, now_ms)?;
        self.admit_policy(policy, root, now_ms)?;
        if value.provenance.tenant_id != value.binding.tenant_id
            || value.provenance.fleet_id != value.binding.fleet_id
            || value.binding != policy.payload.binding
            || value.card.policy_digest != policy.payload.policy.policy_digest
            || value.issued_at_ms > now_ms
            || now_ms >= value.expires_at_ms
            || value.card.capabilities.iter().any(|capability| {
                !registry.contains(capability) || !policy.payload.policy.scopes.contains(capability)
            })
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        Ok(())
    }

    pub fn admit_task(
        &self,
        signed: &DeviceSignedEnvelopeV1<A2aTaskSubmissionV1>,
        root: &FleetTrustRootV1,
        registry: &A2aCapabilityRegistryV1,
        now_ms: u64,
    ) -> Result<(), EnrollmentError> {
        let value = &signed.payload;
        value.task.validate()?;
        let key = self.active_key(&value.binding, now_ms)?;
        signed.verify(&key.public_key, "fleet-a2a-task-submit")?;
        self.admit_signed_card(&value.card, &value.policy, root, registry, now_ms)?;
        if value.registry_revision != registry.revision()
            || value.registry_digest != *registry.digest()
            || value.card.payload.binding != value.binding
            || value.task.mission_revision.policy_digest
                != value.policy.payload.policy.policy_digest
            || value.task.mission_revision.nodes.len()
                > usize::from(value.policy.payload.policy.max_nodes_per_mission)
            || value.task.card_id != value.card.payload.card.card_id
            || value.task.card_revision != value.card.payload.card.revision
            || value.task.state != fleet_protocol::A2ATaskStateV1::Submitted
            || !value
                .card
                .payload
                .card
                .capabilities
                .contains(&value.task.requested_capability)
            || !value
                .policy
                .payload
                .policy
                .scopes
                .contains(&value.task.requested_capability)
            || !registry.contains(&value.task.requested_capability)
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        Ok(())
    }

    pub fn admit_task_command(
        &self,
        signed: &DeviceSignedEnvelopeV1<A2aTaskCommandV1>,
        card: &FleetSignedEnvelopeV1<BoundAgentCardV1>,
        policy: &FleetSignedEnvelopeV1<BoundPolicyEnvelopeV1>,
        root: &FleetTrustRootV1,
        registry: &A2aCapabilityRegistryV1,
        now_ms: u64,
    ) -> Result<(), EnrollmentError> {
        let value = &signed.payload;
        let key = self.active_key(&value.binding, now_ms)?;
        signed.verify(&key.public_key, "fleet-a2a-task-command")?;
        self.admit_signed_card(card, policy, root, registry, now_ms)?;
        if card.payload.binding != value.binding
            || value.card_id != card.payload.card.card_id
            || value.card_revision != card.payload.card.revision
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        Ok(())
    }

    /// Checks a card's active tuple only. This tuple check cannot authorize a connected request;
    /// callers must use [`Self::admit_signed_card`] or [`Self::admit_task`].
    pub fn admit_card(
        &self,
        card: &AgentCardV1,
        binding: &IdentityBindingV1,
        now_ms: u64,
    ) -> Result<(), EnrollmentError> {
        if card.vessel_id != binding.vessel_id
            || card.fleet_node_id != binding.fleet_node_id
            || card.key_id != binding.key_id
            || !self.active(binding, now_ms)
        {
            return Err(EnrollmentError::AdmissionDenied);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ids() -> IdentityBindingV1 {
        IdentityBindingV1 {
            tenant_id: TenantId::new("tenant").unwrap(),
            fleet_id: FleetId::new("fleet").unwrap(),
            vessel_id: VesselId::new("vessel").unwrap(),
            fleet_node_id: FleetNodeId::new("node").unwrap(),
            key_id: KeyId::new("device-k1").unwrap(),
        }
    }
    #[test]
    fn challenge_response_replay_rotation_and_revocation_fail_closed() {
        let binding = ids();
        let device = VesselDeviceKeypairV1::from_private_bytes(binding.key_id.clone(), [3; 32]);
        let mut authority = FleetEnrollmentAuthorityV1::default();
        let challenge = authority
            .issue_challenge(binding.clone(), device.public(), 10, 10)
            .unwrap();
        let response = device.respond(challenge.clone()).unwrap();
        let grant = authority
            .accept_response(response.clone(), 11, 1, 100)
            .unwrap();
        assert_eq!(grant.registration, RegistrationStateV1::Active);
        assert_eq!(
            authority.accept_response(response, 11, 1, 100),
            Err(EnrollmentError::ChallengeDenied)
        );
        let new_key =
            VesselDeviceKeypairV1::from_private_bytes(KeyId::new("device-k2").unwrap(), [4; 32]);
        let mut rotated = binding.clone();
        rotated.key_id = new_key.public().key_id.clone();
        let rotation_response = new_key
            .respond(
                authority
                    .issue_challenge(rotated.clone(), new_key.public(), 12, 10)
                    .unwrap(),
            )
            .unwrap();
        authority
            .rotate(&binding, rotation_response, 13, 2, 100)
            .unwrap();
        assert!(!authority.active(&binding, 12));
        assert!(authority.active(&rotated, 12));
        authority.revoke(&rotated).unwrap();
        assert!(!authority.active(&rotated, 12));
    }

    #[test]
    fn issued_challenges_are_required_and_invalid_responses_do_not_consume_them() {
        let binding = ids();
        let device = VesselDeviceKeypairV1::from_private_bytes(binding.key_id.clone(), [8; 32]);
        let mut authority = FleetEnrollmentAuthorityV1::default();
        let issued = authority
            .issue_challenge(binding.clone(), device.public(), 10, 10)
            .unwrap();
        let response = device.respond(issued.clone()).unwrap();
        let mut invalid = response.clone();
        invalid.device_signature = vec![0; 64];
        assert_eq!(
            authority.accept_response(invalid, 11, 1, 100),
            Err(EnrollmentError::Verification)
        );
        assert!(authority.accept_response(response, 11, 1, 100).is_ok());

        let self_created =
            EnrollmentChallengeV1::new(binding, device.public(), vec![1; 32], 12, 20).unwrap();
        assert_eq!(
            authority.accept_response(device.respond(self_created).unwrap(), 13, 2, 100),
            Err(EnrollmentError::ChallengeDenied)
        );
    }

    #[test]
    fn revoked_registration_cannot_be_reenrolled() {
        let binding = ids();
        let device = VesselDeviceKeypairV1::from_private_bytes(binding.key_id.clone(), [3; 32]);
        let mut authority = FleetEnrollmentAuthorityV1::default();
        let response = device
            .respond(
                authority
                    .issue_challenge(binding.clone(), device.public(), 10, 10)
                    .unwrap(),
            )
            .unwrap();
        authority.accept_response(response, 11, 1, 100).unwrap();
        authority.revoke(&binding).unwrap();
        let replacement =
            VesselDeviceKeypairV1::from_private_bytes(KeyId::new("device-k2").unwrap(), [4; 32]);
        let mut replacement_binding = binding.clone();
        replacement_binding.key_id = replacement.public().key_id.clone();
        let response = replacement
            .respond(
                authority
                    .issue_challenge(replacement_binding, replacement.public(), 12, 10)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            authority.accept_response(response, 13, 2, 100),
            Err(EnrollmentError::KeyDenied)
        );
    }
}
