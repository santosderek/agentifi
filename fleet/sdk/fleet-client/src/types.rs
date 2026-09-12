//! Wire DTOs for Fleet Core's HTTP surface.
//!
//! These mirror the request/response envelopes defined in Fleet Core's `apps/fleet-core/src/lib.rs`.
//! They are declared here, rather than imported, because Core's HTTP DTOs live in the Core binary
//! crate (and its task view lives in `run-store-postgres`, a server-side storage crate a client has
//! no business depending on).
//!
//! The *payloads* inside them are the genuinely shared types from `fleet-enrollment` and
//! `fleet-protocol`, so the parts that carry signatures and identity cannot drift. Only the thin
//! outer request shapes are restated.
//!
//! `deny_unknown_fields` matches Core's own attribute: a field Core does not recognise would be
//! rejected there, so accepting it here would only delay the error.

use fleet_enrollment::{
    A2aTaskCommandV1, BoundAgentCardV1, BoundPolicyEnvelopeV1, CardRegistrationV1,
    DevicePublicKeyV1, DeviceSignedEnvelopeV1, EnrollmentAuthorizationV1, FleetSignedEnvelopeV1,
    IdentityBindingV1,
};
use fleet_protocol::{A2ATaskStateV1, DigestValue, TaskId};
use serde::{Deserialize, Serialize};

/// Body of `POST /v1/enrollment/challenges`.
///
/// The `authorization` is signed by the Fleet root and is supplied by an operator; this crate
/// never produces one. `public_key` is the device's public half only.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueChallengeRequestV1 {
    /// Tenant/fleet/vessel/node/key identity this enrollment is bound to.
    pub binding: IdentityBindingV1,
    /// The device's Ed25519 **public** key.
    pub public_key: DevicePublicKeyV1,
    /// Operator-supplied, Fleet-root-signed authorization for this exact binding and key.
    pub authorization: FleetSignedEnvelopeV1<EnrollmentAuthorizationV1>,
}

/// Body of `POST /v1/a2a/cards`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterCardRequestV1 {
    /// Device-signed card registration.
    pub request: DeviceSignedEnvelopeV1<CardRegistrationV1>,
    /// Fleet-signed policy envelope the card is admitted under.
    pub policy: FleetSignedEnvelopeV1<BoundPolicyEnvelopeV1>,
}

/// Body of the three task-command endpoints: status, cancel, and human-input.
///
/// One shape serves all three because Core distinguishes them by the `operation` inside the signed
/// command, not by the body type. Sending a command whose operation does not match the endpoint is
/// answered with `invalid_request`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCommandRequestV1 {
    /// Device-signed command carrying the operation.
    pub command: DeviceSignedEnvelopeV1<A2aTaskCommandV1>,
    /// Fleet-signed agent card authorising the caller.
    pub card: FleetSignedEnvelopeV1<BoundAgentCardV1>,
    /// Fleet-signed policy envelope.
    pub policy: FleetSignedEnvelopeV1<BoundPolicyEnvelopeV1>,
}

/// Fleet Core's view of a task, returned by submit and the command endpoints.
///
/// Mirrors `run_store_postgres::FleetTaskViewV1`. Restated here so a client does not depend on a
/// server-side storage crate; the field types are all shared protocol types, so a shape change on
/// either side is a deserialisation failure rather than silent misreading.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskViewV1 {
    /// Identifier of the task.
    pub task_id: TaskId,
    /// Current lifecycle state.
    pub state: A2ATaskStateV1,
    /// Digest of the original submission request.
    pub request_digest: DigestValue,
    /// Digest of the task record.
    pub task_digest: DigestValue,
    /// When Core accepted the task.
    pub accepted_at_ms: u64,
    /// When Core last updated it.
    pub updated_at_ms: u64,
}
