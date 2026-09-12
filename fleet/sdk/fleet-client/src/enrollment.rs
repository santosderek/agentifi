//! The two-phase enrollment exchange, sequenced correctly.
//!
//! # Why a helper rather than leaving callers to call the two endpoints
//!
//! Enrollment has ordering and matching requirements that are security-relevant and easy to get
//! subtly wrong. Encoding them once here means every consumer inherits them:
//!
//! * The Fleet trust root must be obtained **out of band** and pinned by the caller *before* the
//!   exchange. Fetching it from the endpoint being authenticated would be circular -- the client
//!   would trust whatever key that endpoint claimed.
//! * The operator's authorization must authorize **exactly** the binding and device public key
//!   being enrolled. Checking locally converts a silent server-side rejection into a precise local
//!   diagnostic, and avoids sending a mismatched artifact at all.
//! * The device private key never participates. Phase one sends the public half; phase two answers
//!   a challenge, which is what proves possession.
//!
//! # What the caller still owns
//!
//! Signing the challenge, and verifying/installing the returned grant against its own pinned root.
//! Those need the device's private key and the caller's own trust store, neither of which this
//! crate touches. The signature is supplied through a closure so the key stays with its owner.

use crate::FleetClient;
use crate::error::FleetClientError;
use crate::types::IssueChallengeRequestV1;
use fleet_enrollment::{
    DevicePublicKeyV1, EnrollmentAuthorizationV1, EnrollmentGrantV1, EnrollmentResponseV1,
    FleetSignedEnvelopeV1, IdentityBindingV1,
};

/// Inputs to one enrollment exchange.
#[derive(Debug)]
pub struct EnrollmentRequest {
    /// Identity this enrollment binds.
    pub binding: IdentityBindingV1,
    /// The device's Ed25519 **public** key. The private half is never passed to this crate.
    pub public_key: DevicePublicKeyV1,
    /// Operator-supplied, Fleet-root-signed authorization.
    pub authorization: FleetSignedEnvelopeV1<EnrollmentAuthorizationV1>,
}

impl FleetClient {
    /// Runs both enrollment phases and returns the Fleet-signed grant.
    ///
    /// `sign_challenge` receives the challenge and must return the device signature over it. It is
    /// a closure so the device private key never crosses this crate's boundary.
    ///
    /// The returned grant is **not** verified here: verification requires the caller's pinned
    /// trust root, and doing it here against a root this crate does not own would be security
    /// theatre. The caller must verify and install it.
    pub async fn enroll<F>(
        &self,
        request: &EnrollmentRequest,
        sign_challenge: F,
    ) -> Result<FleetSignedEnvelopeV1<EnrollmentGrantV1>, FleetClientError>
    where
        F: FnOnce(&fleet_enrollment::EnrollmentChallengeV1) -> Result<Vec<u8>, String>,
    {
        // Local precondition checks. Failing here is strictly better than being rejected remotely:
        // Core answers only `invalid_request`/`denied`, which does not say WHICH field mismatched.
        if request.authorization.payload.binding != request.binding {
            return Err(FleetClientError::CanonicalEncoding(
                "authorization binding does not match the binding being enrolled; the operator \
                 issued this authorization for a different identity"
                    .to_owned(),
            ));
        }
        if request.authorization.payload.public_key.ed25519_public_key
            != request.public_key.ed25519_public_key
        {
            return Err(FleetClientError::CanonicalEncoding(
                "authorization authorizes a different device public key; enrollment would be \
                 rejected by Fleet Core"
                    .to_owned(),
            ));
        }

        // Phase one: obtain a challenge.
        let challenge = self
            .issue_challenge(&IssueChallengeRequestV1 {
                binding: request.binding.clone(),
                public_key: request.public_key.clone(),
                authorization: request.authorization.clone(),
            })
            .await?;

        // Phase two: prove possession without disclosing the key.
        let device_signature =
            sign_challenge(&challenge).map_err(|message| FleetClientError::Credential {
                path: "<device key>".to_owned(),
                message,
            })?;

        self.accept_enrollment(&EnrollmentResponseV1 {
            challenge,
            device_signature,
        })
        .await
    }
}
