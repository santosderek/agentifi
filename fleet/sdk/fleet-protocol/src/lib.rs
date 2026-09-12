//! Bounded, transport-neutral Fleet/Vessel protocol records.
//!
//! Records are canonical JSON with a purpose-separated digest.  The protocol deliberately has no
//! database, runtime, transport, or cryptography dependency; signatures are implemented by an
//! outer adapter.

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use thiserror::Error;

pub const SCHEMA_VERSION_V1: u32 = 1;
pub const MAX_RECORD_BYTES: usize = 256 * 1024;
pub const MAX_BATCH_RECORDS: usize = 256;
pub const MAX_MISSION_NODES: usize = 256;
pub const MAX_NODE_DEPENDENCIES: usize = 64;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_JSON_DEPTH: usize = 32;
pub const MAX_JSON_NODES: usize = 10_000;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("{0} must contain 1..={1} safe UTF-8 bytes")]
    InvalidText(&'static str, usize),
    #[error("invalid sha256 digest")]
    InvalidDigest,
    #[error("unsupported schema version")]
    UnsupportedVersion,
    #[error("record exceeds its byte limit")]
    TooLarge,
    #[error("invalid or noncanonical JSON")]
    NonCanonical,
    #[error("duplicate JSON object key")]
    DuplicateKey,
    #[error("floating point JSON is forbidden")]
    FloatForbidden,
    #[error("JSON structural limit exceeded")]
    StructuralLimit,
    #[error("invalid protocol invariant: {0}")]
    InvalidInvariant(&'static str),
    #[error("record digest mismatch")]
    DigestMismatch,
}

fn valid_id(value: &str, route: bool) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b'.')
                || (!route && byte == b':')
        })
        && !value.contains("..")
}

macro_rules! id_type {
    ($($name:ident, $field:literal);+ $(;)?) => {$ (
        #[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
                let value = value.into();
                if !valid_id(&value, false) { return Err(ProtocolError::InvalidText($field, 128)); }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str { &self.0 }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
            }
        }
    )+ };
}

id_type!(
    FleetId, "fleet_id";
    TenantId, "tenant_id";
    FleetNodeId, "fleet_node_id";
    VesselId, "vessel_id";
    PrincipalId, "principal_id";
    AgentId, "agent_id";
    AgentCardId, "agent_card_id";
    TaskId, "task_id";
    MissionId, "mission_id";
    MissionRevisionId, "mission_revision_id";
    MissionExecutionId, "mission_execution_id";
    MissionNodeId, "mission_node_id";
    RecordId, "record_id";
    CorrelationId, "correlation_id";
    CausationId, "causation_id";
    RoomId, "room_id";
    MessageId, "message_id";
    StreamId, "stream_id";
    CheckpointId, "checkpoint_id";
    ArtifactId, "artifact_id";
    OfficerId, "officer_id";
    IncidentId, "incident_id";
    PolicyId, "policy_id";
    CrewMemberId, "crew_member_id";
    CommandId, "command_id";
    KeyId, "key_id"
);

#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct DigestValue(String);
impl DigestValue {
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(ProtocolError::InvalidDigest);
        };
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ProtocolError::InvalidDigest);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for DigestValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

pub fn digest(purpose: &str, bytes: &[u8]) -> Result<DigestValue, ProtocolError> {
    if !valid_id(purpose, false) {
        return Err(ProtocolError::InvalidText("digest purpose", 128));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"fleet-protocol-v1\0");
    hasher.update((purpose.len() as u64).to_be_bytes());
    hasher.update(purpose.as_bytes());
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    DigestValue::new(format!("sha256:{:x}", hasher.finalize()))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct HybridLogicalClockV1 {
    pub physical_ms: u64,
    pub logical: u32,
    pub node_id: FleetNodeId,
}
impl HybridLogicalClockV1 {
    pub fn next(&self, physical_ms: u64) -> Result<Self, ProtocolError> {
        if physical_ms > self.physical_ms {
            Ok(Self {
                physical_ms,
                logical: 0,
                node_id: self.node_id.clone(),
            })
        } else {
            Ok(Self {
                physical_ms: self.physical_ms,
                logical: self
                    .logical
                    .checked_add(1)
                    .ok_or(ProtocolError::InvalidInvariant(
                        "HLC logical counter overflow",
                    ))?,
                node_id: self.node_id.clone(),
            })
        }
    }
    pub fn merge(&self, remote: &Self, physical_ms: u64) -> Result<Self, ProtocolError> {
        let maximum = self.physical_ms.max(remote.physical_ms).max(physical_ms);
        let logical = if maximum == self.physical_ms && maximum == remote.physical_ms {
            self.logical.max(remote.logical).checked_add(1)
        } else if maximum == self.physical_ms {
            self.logical.checked_add(1)
        } else if maximum == remote.physical_ms {
            remote.logical.checked_add(1)
        } else {
            Some(0)
        }
        .ok_or(ProtocolError::InvalidInvariant(
            "HLC logical counter overflow",
        ))?;
        Ok(Self {
            physical_ms: maximum,
            logical,
            node_id: self.node_id.clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipV1 {
    VesselLocal,
    FleetOwned,
    SharedAppendOnly,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStateV1 {
    None,
    Quarantined,
    ReconciliationRequired,
    HumanDispositionRequired,
    Resolved,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnershipConflictV1 {
    pub conflict_id: RecordId,
    pub subject_digest: DigestValue,
    pub local_owner: OwnershipV1,
    pub asserted_remote_owner: OwnershipV1,
    pub state: ConflictStateV1,
    pub rationale: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceV1 {
    pub origin_node_id: FleetNodeId,
    pub principal_id: PrincipalId,
    pub trust_domain: String,
    pub source_record_digest: Option<DigestValue>,
}
impl ProvenanceV1 {
    fn validate(&self) -> Result<(), ProtocolError> {
        if !valid_id(&self.trust_domain, true) {
            return Err(ProtocolError::InvalidText("trust_domain", 128));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecordHeaderV1 {
    pub schema_version: u32,
    pub record_id: RecordId,
    pub tenant_id: TenantId,
    pub fleet_id: FleetId,
    pub vessel_id: Option<VesselId>,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<CausationId>,
    pub hlc: HybridLogicalClockV1,
    pub local_sequence: u64,
    pub ownership: OwnershipV1,
    pub conflict_state: ConflictStateV1,
    pub provenance: ProvenanceV1,
}
impl RecordHeaderV1 {
    fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != SCHEMA_VERSION_V1 || self.local_sequence == 0 {
            return Err(ProtocolError::UnsupportedVersion);
        }
        self.provenance.validate()?;
        if self.hlc.node_id != self.provenance.origin_node_id {
            return Err(ProtocolError::InvalidInvariant(
                "HLC node must equal provenance origin",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FleetIdentityV1 {
    pub fleet_id: FleetId,
    pub display_name: String,
    pub authority_revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VesselIdentityV1 {
    pub vessel_id: VesselId,
    pub fleet_node_id: FleetNodeId,
    pub identity_revision: u64,
    pub public_key_id: KeyId,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationStateV1 {
    Pending,
    Active,
    Suspended,
    Revoked,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VesselRegistrationV1 {
    pub vessel: VesselIdentityV1,
    pub state: RegistrationStateV1,
    pub card_revision: u64,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PolicyEnvelopeV1 {
    pub policy_id: PolicyId,
    pub revision: u64,
    pub vessel_id: VesselId,
    pub not_before_ms: u64,
    pub expires_at_ms: u64,
    pub offline_allowed: bool,
    pub scopes: BTreeSet<String>,
    pub max_concurrency: u16,
    pub max_nodes_per_mission: u16,
    pub policy_digest: DigestValue,
}
impl PolicyEnvelopeV1 {
    #[allow(clippy::too_many_arguments, clippy::items_after_statements)]
    pub fn new(
        policy_id: PolicyId,
        revision: u64,
        vessel_id: VesselId,
        not_before_ms: u64,
        expires_at_ms: u64,
        offline_allowed: bool,
        scopes: BTreeSet<String>,
        max_concurrency: u16,
        max_nodes_per_mission: u16,
    ) -> Result<Self, ProtocolError> {
        if revision == 0
            || not_before_ms >= expires_at_ms
            || max_concurrency == 0
            || max_concurrency > 64
            || max_nodes_per_mission == 0
            || usize::from(max_nodes_per_mission) > MAX_MISSION_NODES
            || scopes.is_empty()
            || scopes.len() > 64
            || scopes.iter().any(|scope| !valid_id(scope, true))
        {
            return Err(ProtocolError::InvalidInvariant("invalid policy envelope"));
        }
        #[derive(Serialize)]
        struct Body<'a> {
            policy_id: &'a PolicyId,
            revision: u64,
            vessel_id: &'a VesselId,
            not_before_ms: u64,
            expires_at_ms: u64,
            offline_allowed: bool,
            scopes: &'a BTreeSet<String>,
            max_concurrency: u16,
            max_nodes_per_mission: u16,
        }
        let body = Body {
            policy_id: &policy_id,
            revision,
            vessel_id: &vessel_id,
            not_before_ms,
            expires_at_ms,
            offline_allowed,
            scopes: &scopes,
            max_concurrency,
            max_nodes_per_mission,
        };
        let bytes = serde_json::to_vec(&body).map_err(|_| ProtocolError::NonCanonical)?;
        let policy_digest = digest("policy-envelope", &bytes)?;
        Ok(Self {
            policy_id,
            revision,
            vessel_id,
            not_before_ms,
            expires_at_ms,
            offline_allowed,
            scopes,
            max_concurrency,
            max_nodes_per_mission,
            policy_digest,
        })
    }
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let rebuilt = Self::new(
            self.policy_id.clone(),
            self.revision,
            self.vessel_id.clone(),
            self.not_before_ms,
            self.expires_at_ms,
            self.offline_allowed,
            self.scopes.clone(),
            self.max_concurrency,
            self.max_nodes_per_mission,
        )?;
        if rebuilt != *self {
            return Err(ProtocolError::DigestMismatch);
        }
        Ok(())
    }
    pub fn permits(&self, scope: &str, now_ms: u64) -> bool {
        self.offline_allowed
            && self.not_before_ms <= now_ms
            && now_ms < self.expires_at_ms
            && self.scopes.contains(scope)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentCardV1 {
    pub card_id: AgentCardId,
    pub agent_id: AgentId,
    pub revision: u64,
    pub vessel_id: VesselId,
    /// Fleet-node/key binding is required for connected admission; an Agent Card never grants it.
    pub fleet_node_id: FleetNodeId,
    pub key_id: KeyId,
    pub capabilities: BTreeSet<String>,
    pub policy_digest: DigestValue,
}
impl AgentCardV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.revision == 0
            || self.capabilities.is_empty()
            || self.capabilities.len() > 64
            || self.capabilities.iter().any(|item| !valid_id(item, true))
        {
            return Err(ProtocolError::InvalidInvariant("invalid Agent Card"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum A2ATaskStateV1 {
    Submitted,
    Accepted,
    Running,
    InputRequired,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct A2ATaskV1 {
    pub task_id: TaskId,
    pub card_id: AgentCardId,
    pub card_revision: u64,
    /// The submitted immutable DAG must be persisted before this task can become accepted.
    pub mission_revision: MissionRevisionV1,
    pub requested_capability: String,
    pub canonical_input: Vec<u8>,
    pub state: A2ATaskStateV1,
}
impl A2ATaskV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.card_revision == 0
            || !valid_id(&self.requested_capability, true)
            || self.canonical_input.len() > 64 * 1024
            || validate_canonical_json_bytes(&self.canonical_input).is_err()
            || self.mission_revision.validate().is_err()
        {
            return Err(ProtocolError::InvalidInvariant("invalid A2A task"));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| ProtocolError::NonCanonical)?;
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        Ok(bytes)
    }
    pub fn digest(&self) -> Result<DigestValue, ProtocolError> {
        digest("fleet-a2a-task", &self.canonical_bytes()?)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum A2ATaskActionV1 {
    Status {
        state: A2ATaskStateV1,
        diagnostic: Option<String>,
    },
    Cancel {
        reason: String,
    },
    HumanInput {
        prompt: String,
        response: Option<String>,
    },
}
impl A2ATaskActionV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Status { diagnostic, .. }
                if diagnostic.as_ref().is_some_and(|text| text.len() > 4096) =>
            {
                Err(ProtocolError::InvalidInvariant("invalid task status"))
            }
            Self::Cancel { reason } if reason.is_empty() || reason.len() > 1024 => {
                Err(ProtocolError::InvalidInvariant("invalid cancellation"))
            }
            Self::HumanInput { prompt, response }
                if prompt.is_empty()
                    || prompt.len() > 4096
                    || response
                        .as_ref()
                        .is_some_and(|text| text.len() > MAX_TEXT_BYTES) =>
            {
                Err(ProtocolError::InvalidInvariant("invalid human input"))
            }
            _ => Ok(()),
        }
    }
    pub fn digest(&self) -> Result<DigestValue, ProtocolError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| ProtocolError::NonCanonical)?;
        digest("fleet-a2a-task-action", &bytes)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeEffectV1 {
    PureModel,
    ExternallyIdempotent,
    ExternalUnknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MissionNodeV1 {
    pub node_id: MissionNodeId,
    pub agent_id: AgentId,
    pub prompt: String,
    pub dependencies: Vec<MissionNodeId>,
    pub max_attempts: u16,
    pub effect: NodeEffectV1,
}

/// Immutable Mission identity. A definition owns no mutable execution state; every DAG is
/// carried by one separately immutable [`MissionRevisionV1`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MissionDefinitionV1 {
    pub mission_id: MissionId,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MissionRevisionV1 {
    pub mission_id: MissionId,
    pub revision_id: MissionRevisionId,
    pub revision: u64,
    pub policy_digest: DigestValue,
    pub nodes: Vec<MissionNodeV1>,
    pub revision_digest: DigestValue,
}
impl MissionRevisionV1 {
    #[allow(clippy::items_after_statements)]
    pub fn new(
        mission_id: MissionId,
        revision_id: MissionRevisionId,
        revision: u64,
        policy_digest: DigestValue,
        mut nodes: Vec<MissionNodeV1>,
    ) -> Result<Self, ProtocolError> {
        nodes.sort_by(|left, right| left.node_id.cmp(&right.node_id));
        for node in &mut nodes {
            node.dependencies.sort();
        }
        validate_nodes(&nodes)?;
        if revision == 0 {
            return Err(ProtocolError::InvalidInvariant(
                "mission revision must be positive",
            ));
        }
        #[derive(Serialize)]
        struct Body<'a> {
            mission_id: &'a MissionId,
            revision_id: &'a MissionRevisionId,
            revision: u64,
            policy_digest: &'a DigestValue,
            nodes: &'a [MissionNodeV1],
        }
        let bytes = serde_json::to_vec(&Body {
            mission_id: &mission_id,
            revision_id: &revision_id,
            revision,
            policy_digest: &policy_digest,
            nodes: &nodes,
        })
        .map_err(|_| ProtocolError::NonCanonical)?;
        let revision_digest = digest("mission-revision", &bytes)?;
        Ok(Self {
            mission_id,
            revision_id,
            revision,
            policy_digest,
            nodes,
            revision_digest,
        })
    }
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let rebuilt = Self::new(
            self.mission_id.clone(),
            self.revision_id.clone(),
            self.revision,
            self.policy_digest.clone(),
            self.nodes.clone(),
        )?;
        if rebuilt != *self {
            return Err(ProtocolError::DigestMismatch);
        }
        Ok(())
    }
    pub fn deterministic_topological_order(&self) -> Result<Vec<MissionNodeId>, ProtocolError> {
        self.validate()?;
        let mut done = BTreeSet::new();
        let mut order = Vec::with_capacity(self.nodes.len());
        while order.len() < self.nodes.len() {
            let Some(node) = self.nodes.iter().find(|node| {
                !done.contains(&node.node_id)
                    && node
                        .dependencies
                        .iter()
                        .all(|dependency| done.contains(dependency))
            }) else {
                return Err(ProtocolError::InvalidInvariant(
                    "mission revision contains a cycle",
                ));
            };
            done.insert(node.node_id.clone());
            order.push(node.node_id.clone());
        }
        Ok(order)
    }
}

fn validate_nodes(nodes: &[MissionNodeV1]) -> Result<(), ProtocolError> {
    if nodes.is_empty() || nodes.len() > MAX_MISSION_NODES {
        return Err(ProtocolError::InvalidInvariant(
            "invalid mission node count",
        ));
    }
    if nodes
        .windows(2)
        .any(|pair| pair[0].node_id >= pair[1].node_id)
    {
        return Err(ProtocolError::InvalidInvariant(
            "nodes must be uniquely ordered",
        ));
    }
    let ids: BTreeSet<_> = nodes.iter().map(|node| node.node_id.clone()).collect();
    for node in nodes {
        if node.prompt.is_empty()
            || node.prompt.len() > MAX_TEXT_BYTES
            || node.max_attempts == 0
            || node.max_attempts > 16
            || node.dependencies.len() > MAX_NODE_DEPENDENCIES
            || node.dependencies.windows(2).any(|pair| pair[0] >= pair[1])
            || node
                .dependencies
                .iter()
                .any(|dependency| dependency == &node.node_id || !ids.contains(dependency))
        {
            return Err(ProtocolError::InvalidInvariant("invalid mission node"));
        }
    }
    let temporary = MissionRevisionV1 {
        mission_id: MissionId::new("validation")?,
        revision_id: MissionRevisionId::new("validation")?,
        revision: 1,
        policy_digest: digest("validation", b"validation")?,
        nodes: nodes.to_vec(),
        revision_digest: digest("validation", b"graph")?,
    };
    let mut done = BTreeSet::new();
    while done.len() < nodes.len() {
        let before = done.len();
        for node in &temporary.nodes {
            if node
                .dependencies
                .iter()
                .all(|dependency| done.contains(dependency))
            {
                done.insert(node.node_id.clone());
            }
        }
        if done.len() == before {
            return Err(ProtocolError::InvalidInvariant(
                "mission revision contains a cycle",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionNodeStateV1 {
    Pending,
    Running,
    RetryWait,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MissionEventV1 {
    pub mission_id: MissionId,
    pub revision_id: MissionRevisionId,
    pub execution_id: MissionExecutionId,
    pub node_id: Option<MissionNodeId>,
    pub attempt: Option<u16>,
    pub state: MissionNodeStateV1,
    pub diagnostic_code: String,
    pub output_digest: Option<DigestValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoomMessageV1 {
    pub room_id: RoomId,
    pub message_id: MessageId,
    pub author: PrincipalId,
    pub body: String,
    pub correction_of: Option<MessageId>,
    pub membership_revision: u64,
}
impl RoomMessageV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.body.is_empty() || self.body.len() > MAX_TEXT_BYTES || self.membership_revision == 0
        {
            return Err(ProtocolError::InvalidInvariant("invalid room message"));
        }
        Ok(())
    }
}

/// Fleet-controlled, immutable membership for the single bootstrap Fleet Room.  A Vessel may
/// cache this record but cannot mint or alter membership from an offline message.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FleetRoomMembershipV1 {
    pub room_id: RoomId,
    pub membership_revision: u64,
    pub members: BTreeSet<PrincipalId>,
    pub expires_at_ms: u64,
}
impl FleetRoomMembershipV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.membership_revision == 0
            || self.members.is_empty()
            || self.members.len() > 256
            || self.expires_at_ms == 0
        {
            return Err(ProtocolError::InvalidInvariant(
                "invalid Fleet Room membership",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OfficerManifestV1 {
    pub officer_id: OfficerId,
    pub kind: OfficerKindV1,
    pub stream_id: StreamId,
    pub max_findings_per_window: u16,
    pub cooldown_ms: u64,
    pub observe_only: bool,
}
impl OfficerManifestV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.max_findings_per_window == 0 || self.cooldown_ms == 0 || !self.observe_only {
            return Err(ProtocolError::InvalidInvariant(
                "bootstrap Officers are observe/recommend-only",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OfficerFindingV1 {
    pub officer_id: OfficerId,
    pub kind: OfficerKindV1,
    pub incident_id: IncidentId,
    pub fingerprint: DigestValue,
    pub source_record_digest: DigestValue,
    pub severity: u8,
    pub observation: String,
    pub recommendation: Option<String>,
    pub observe_only: bool,
}
impl OfficerFindingV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        OfficerRecordV1 {
            officer_id: self.officer_id.clone(),
            kind: self.kind,
            incident_id: self.incident_id.clone(),
            source_record_digest: self.source_record_digest.clone(),
            severity: self.severity,
            observation: self.observation.clone(),
            recommendation: self.recommendation.clone(),
            observe_only: self.observe_only,
        }
        .validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SyncManifestV1 {
    pub vessel_id: VesselId,
    pub supported_schema_versions: BTreeSet<u32>,
    pub checkpoints: BTreeMap<StreamId, u64>,
    pub max_batch_records: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SyncBatchV1 {
    pub stream_id: StreamId,
    pub from_sequence: u64,
    pub to_sequence: u64,
    pub record_digests: Vec<DigestValue>,
    pub previous_batch_digest: Option<DigestValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SyncCheckpointV1 {
    pub checkpoint_id: CheckpointId,
    pub stream_id: StreamId,
    pub durable_sequence: u64,
    pub last_record_digest: DigestValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactManifestV1 {
    pub artifact_id: ArtifactId,
    pub media_type: String,
    pub byte_length: u64,
    pub content_digest: DigestValue,
    pub chunk_size: u32,
    pub chunk_digests: Vec<DigestValue>,
}
impl ArtifactManifestV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let expected_chunks = self.byte_length.div_ceil(u64::from(self.chunk_size.max(1)));
        if self.byte_length == 0
            || self.chunk_size == 0
            || self.chunk_size > 4 * 1024 * 1024
            || self.chunk_digests.is_empty()
            || self.chunk_digests.len() > 4096
            || u64::try_from(self.chunk_digests.len()).ok() != Some(expected_chunks)
            || self.media_type.is_empty()
            || self.media_type.len() > 128
        {
            return Err(ProtocolError::InvalidInvariant("invalid artifact manifest"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OfficerKindV1 {
    Security,
    FleetHealth,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OfficerRecordV1 {
    pub officer_id: OfficerId,
    pub kind: OfficerKindV1,
    pub incident_id: IncidentId,
    pub source_record_digest: DigestValue,
    pub severity: u8,
    pub observation: String,
    pub recommendation: Option<String>,
    pub observe_only: bool,
}
impl OfficerRecordV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.severity > 10
            || self.observation.is_empty()
            || self.observation.len() > MAX_TEXT_BYTES
            || self
                .recommendation
                .as_ref()
                .is_some_and(|text| text.len() > MAX_TEXT_BYTES)
            || !self.observe_only
        {
            return Err(ProtocolError::InvalidInvariant(
                "bootstrap Officers are observe-only",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "record_type", content = "record", rename_all = "snake_case")]
pub enum FleetRecordPayloadV1 {
    FleetIdentity(FleetIdentityV1),
    VesselIdentity(VesselIdentityV1),
    VesselRegistration(VesselRegistrationV1),
    PolicyEnvelope(PolicyEnvelopeV1),
    AgentCard(AgentCardV1),
    A2ATask(A2ATaskV1),
    A2ATaskAction(A2ATaskActionV1),
    MissionDefinition(MissionDefinitionV1),
    MissionRevision(MissionRevisionV1),
    MissionEvent(MissionEventV1),
    RoomMessage(RoomMessageV1),
    FleetRoomMembership(FleetRoomMembershipV1),
    OfficerManifest(OfficerManifestV1),
    OfficerFinding(OfficerFindingV1),
    SyncManifest(SyncManifestV1),
    SyncBatch(SyncBatchV1),
    SyncCheckpoint(SyncCheckpointV1),
    ArtifactManifest(ArtifactManifestV1),
    OfficerRecord(OfficerRecordV1),
    OwnershipConflict(OwnershipConflictV1),
}
impl FleetRecordPayloadV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::PolicyEnvelope(value) => value.validate(),
            Self::MissionRevision(value) => value.validate(),
            Self::FleetIdentity(value)
                if value.display_name.is_empty()
                    || value.display_name.len() > 256
                    || value.authority_revision == 0 =>
            {
                Err(ProtocolError::InvalidInvariant("invalid Fleet identity"))
            }
            Self::VesselIdentity(value) if value.identity_revision == 0 => Err(
                ProtocolError::InvalidInvariant("invalid Vessel identity revision"),
            ),
            Self::VesselRegistration(value)
                if value.card_revision == 0 || value.expires_at_ms == 0 =>
            {
                Err(ProtocolError::InvalidInvariant(
                    "invalid Vessel registration",
                ))
            }
            Self::AgentCard(value) => value.validate(),
            Self::A2ATask(value) => value.validate(),
            Self::MissionEvent(value)
                if value.diagnostic_code.len() > 128 || !valid_id(&value.diagnostic_code, true) =>
            {
                Err(ProtocolError::InvalidInvariant("invalid Mission event"))
            }
            Self::RoomMessage(value) => value.validate(),
            Self::FleetRoomMembership(value) => value.validate(),
            Self::OfficerManifest(value) => value.validate(),
            Self::OfficerFinding(value) => value.validate(),
            Self::SyncManifest(value)
                if !value.supported_schema_versions.contains(&SCHEMA_VERSION_V1)
                    || value.checkpoints.len() > 128
                    || value.max_batch_records == 0
                    || usize::from(value.max_batch_records) > MAX_BATCH_RECORDS =>
            {
                Err(ProtocolError::InvalidInvariant("invalid sync manifest"))
            }
            Self::SyncBatch(value)
                if value.from_sequence == 0
                    || value.to_sequence < value.from_sequence
                    || value.record_digests.is_empty()
                    || value.record_digests.len() > MAX_BATCH_RECORDS
                    || value.record_digests.iter().collect::<BTreeSet<_>>().len()
                        != value.record_digests.len()
                    || value.to_sequence - value.from_sequence + 1
                        != value.record_digests.len() as u64 =>
            {
                Err(ProtocolError::InvalidInvariant("invalid sync batch"))
            }
            Self::SyncCheckpoint(value) if value.durable_sequence == 0 => {
                Err(ProtocolError::InvalidInvariant("invalid sync checkpoint"))
            }
            Self::ArtifactManifest(value) => value.validate(),
            Self::OfficerRecord(value) => value.validate(),
            Self::A2ATaskAction(A2ATaskActionV1::Status { diagnostic, .. })
                if diagnostic.as_ref().is_some_and(|text| text.len() > 4096) =>
            {
                Err(ProtocolError::InvalidInvariant("invalid task status"))
            }
            Self::A2ATaskAction(A2ATaskActionV1::Cancel { reason })
                if reason.is_empty() || reason.len() > 1024 =>
            {
                Err(ProtocolError::InvalidInvariant("invalid cancellation"))
            }
            Self::A2ATaskAction(A2ATaskActionV1::HumanInput { prompt, response })
                if prompt.is_empty()
                    || prompt.len() > 4096
                    || response
                        .as_ref()
                        .is_some_and(|text| text.len() > MAX_TEXT_BYTES) =>
            {
                Err(ProtocolError::InvalidInvariant("invalid human input"))
            }
            Self::OwnershipConflict(value)
                if value.state == ConflictStateV1::None
                    || value.rationale.is_empty()
                    || value.rationale.len() > 4096 =>
            {
                Err(ProtocolError::InvalidInvariant(
                    "invalid ownership conflict",
                ))
            }
            _ => Ok(()),
        }
    }
}

fn validate_header_payload(
    header: &RecordHeaderV1,
    payload: &FleetRecordPayloadV1,
) -> Result<(), ProtocolError> {
    let ownership = match payload {
        FleetRecordPayloadV1::FleetIdentity(value) => {
            if value.fleet_id != header.fleet_id || header.vessel_id.is_some() {
                return Err(ProtocolError::InvalidInvariant(
                    "Fleet identity header mismatch",
                ));
            }
            OwnershipV1::FleetOwned
        }
        FleetRecordPayloadV1::VesselIdentity(value) => {
            if header.vessel_id.as_ref() != Some(&value.vessel_id) {
                return Err(ProtocolError::InvalidInvariant(
                    "Vessel identity header mismatch",
                ));
            }
            OwnershipV1::VesselLocal
        }
        FleetRecordPayloadV1::VesselRegistration(value) => {
            if header.vessel_id.as_ref() != Some(&value.vessel.vessel_id) {
                return Err(ProtocolError::InvalidInvariant(
                    "Vessel registration header mismatch",
                ));
            }
            OwnershipV1::FleetOwned
        }
        FleetRecordPayloadV1::PolicyEnvelope(value) => {
            if header.vessel_id.as_ref() != Some(&value.vessel_id) {
                return Err(ProtocolError::InvalidInvariant("policy header mismatch"));
            }
            OwnershipV1::FleetOwned
        }
        FleetRecordPayloadV1::AgentCard(value) => {
            if header.vessel_id.as_ref() != Some(&value.vessel_id) {
                return Err(ProtocolError::InvalidInvariant(
                    "Agent Card header mismatch",
                ));
            }
            OwnershipV1::FleetOwned
        }
        FleetRecordPayloadV1::A2ATask(_)
        | FleetRecordPayloadV1::A2ATaskAction(_)
        | FleetRecordPayloadV1::RoomMessage(_)
        | FleetRecordPayloadV1::OfficerFinding(_)
        | FleetRecordPayloadV1::SyncBatch(_)
        | FleetRecordPayloadV1::SyncCheckpoint(_)
        | FleetRecordPayloadV1::OfficerRecord(_)
        | FleetRecordPayloadV1::OwnershipConflict(_) => OwnershipV1::SharedAppendOnly,
        FleetRecordPayloadV1::FleetRoomMembership(_) | FleetRecordPayloadV1::OfficerManifest(_) => {
            OwnershipV1::FleetOwned
        }
        FleetRecordPayloadV1::MissionDefinition(_)
        | FleetRecordPayloadV1::MissionRevision(_)
        | FleetRecordPayloadV1::MissionEvent(_)
        | FleetRecordPayloadV1::ArtifactManifest(_) => {
            if header.vessel_id.is_none() {
                return Err(ProtocolError::InvalidInvariant(
                    "Vessel-local record requires Vessel identity",
                ));
            }
            OwnershipV1::VesselLocal
        }
        FleetRecordPayloadV1::SyncManifest(value) => {
            if header.vessel_id.as_ref() != Some(&value.vessel_id) {
                return Err(ProtocolError::InvalidInvariant(
                    "sync manifest header mismatch",
                ));
            }
            OwnershipV1::SharedAppendOnly
        }
    };
    if header.ownership != ownership {
        return Err(ProtocolError::InvalidInvariant(
            "record ownership does not match payload family",
        ));
    }
    if let FleetRecordPayloadV1::OwnershipConflict(value) = payload {
        if header.conflict_state != value.state {
            return Err(ProtocolError::InvalidInvariant("conflict state mismatch"));
        }
    } else if header.conflict_state != ConflictStateV1::None {
        return Err(ProtocolError::InvalidInvariant(
            "non-conflict record carries conflict state",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FleetRecordV1 {
    pub header: RecordHeaderV1,
    pub payload: FleetRecordPayloadV1,
    pub digest: DigestValue,
}
impl FleetRecordV1 {
    pub fn new(
        header: RecordHeaderV1,
        payload: FleetRecordPayloadV1,
    ) -> Result<Self, ProtocolError> {
        header.validate()?;
        payload.validate()?;
        validate_header_payload(&header, &payload)?;
        let bytes = canonical_body(&header, &payload)?;
        let digest = digest("fleet-record", &bytes)?;
        let record = Self {
            header,
            payload,
            digest,
        };
        if record.canonical_bytes()?.len() > MAX_RECORD_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        Ok(record)
    }
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.header.validate()?;
        self.payload.validate()?;
        validate_header_payload(&self.header, &self.payload)?;
        if digest(
            "fleet-record",
            &canonical_body(&self.header, &self.payload)?,
        )? != self.digest
        {
            return Err(ProtocolError::DigestMismatch);
        }
        if self.canonical_bytes()?.len() > MAX_RECORD_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        serde_json::to_vec(self).map_err(|_| ProtocolError::NonCanonical)
    }
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        parse_bounded_json(bytes)?;
        let record: Self =
            serde_json::from_slice(bytes).map_err(|_| ProtocolError::NonCanonical)?;
        record.validate()?;
        if record.canonical_bytes()? != bytes {
            return Err(ProtocolError::NonCanonical);
        }
        Ok(record)
    }
}
fn canonical_body(
    header: &RecordHeaderV1,
    payload: &FleetRecordPayloadV1,
) -> Result<Vec<u8>, ProtocolError> {
    #[derive(Serialize)]
    struct Body<'a> {
        header: &'a RecordHeaderV1,
        payload: &'a FleetRecordPayloadV1,
    }
    serde_json::to_vec(&Body { header, payload }).map_err(|_| ProtocolError::NonCanonical)
}

#[derive(Default)]
struct Shape {
    nodes: usize,
}
fn check_shape(value: &Value, depth: usize, shape: &mut Shape) -> Result<(), ProtocolError> {
    if depth > MAX_JSON_DEPTH {
        return Err(ProtocolError::StructuralLimit);
    }
    shape.nodes = shape
        .nodes
        .checked_add(1)
        .ok_or(ProtocolError::StructuralLimit)?;
    if shape.nodes > MAX_JSON_NODES {
        return Err(ProtocolError::StructuralLimit);
    }
    match value {
        Value::Number(number) if !(number.is_i64() || number.is_u64()) => {
            Err(ProtocolError::FloatForbidden)
        }
        Value::String(text) if text.len() > MAX_TEXT_BYTES => Err(ProtocolError::StructuralLimit),
        Value::Array(items) if items.len() > 4096 => Err(ProtocolError::StructuralLimit),
        Value::Object(items) if items.len() > 256 => Err(ProtocolError::StructuralLimit),
        Value::Array(items) => {
            for item in items {
                check_shape(item, depth + 1, shape)?;
            }
            Ok(())
        }
        Value::Object(items) => {
            for (key, item) in items {
                if key.len() > 256 {
                    return Err(ProtocolError::StructuralLimit);
                }
                check_shape(item, depth + 1, shape)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
struct NoDuplicate;
impl<'de> Visitor<'de> for NoDuplicate {
    type Value = Value;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded JSON")
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Value, E> {
        Err(E::custom("floating point forbidden"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }
    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(NoDuplicateSeed)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate key"));
            }
            values.insert(key, map.next_value_seed(NoDuplicateSeed)?);
        }
        Ok(Value::Object(values))
    }
}
struct NoDuplicateSeed;
impl<'de> de::DeserializeSeed<'de> for NoDuplicateSeed {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(NoDuplicate)
    }
}
/// Validates bounded, integer-only canonical JSON and rejects duplicate object keys.
///
/// Transport adapters must call this before deserializing an authority-bearing request.
pub fn validate_canonical_json_bytes(bytes: &[u8]) -> Result<(), ProtocolError> {
    if bytes.len() > 64 * 1024 {
        return Err(ProtocolError::TooLarge);
    }
    let value = parse_bounded_json(bytes)?;
    if serde_json::to_vec(&value).map_err(|_| ProtocolError::NonCanonical)? != bytes {
        return Err(ProtocolError::NonCanonical);
    }
    Ok(())
}

fn parse_bounded_json(bytes: &[u8]) -> Result<Value, ProtocolError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = NoDuplicateSeed
        .deserialize(&mut deserializer)
        .map_err(|error| {
            let text = error.to_string();
            if text.contains("duplicate") {
                ProtocolError::DuplicateKey
            } else if text.contains("floating point") {
                ProtocolError::FloatForbidden
            } else {
                ProtocolError::NonCanonical
            }
        })?;
    deserializer
        .end()
        .map_err(|_| ProtocolError::NonCanonical)?;
    check_shape(&value, 1, &mut Shape::default())?;
    Ok(value)
}
