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
fn mission(policy: &PolicyEnvelopeV1, id: &str) -> MissionRevisionV1 {
    MissionRevisionV1::new(
        MissionId::new(id).unwrap(),
        MissionRevisionId::new(format!("{id}-r1")).unwrap(),
        1,
        policy.policy_digest.clone(),
        vec![MissionNodeV1 {
            node_id: MissionNodeId::new("node-1").unwrap(),
            agent_id: AgentId::new("agent-a").unwrap(),
            prompt: "fixture".into(),
            dependencies: vec![],
            max_attempts: 1,
            effect: NodeEffectV1::PureModel,
        }],
    )
    .unwrap()
}
fn policy() -> PolicyEnvelopeV1 {
    PolicyEnvelopeV1::new(
        PolicyId::new("policy-a").unwrap(),
        1,
        VesselId::new("vessel-a").unwrap(),
        10,
        100,
        true,
        BTreeSet::from(["mission.execute.offline".into()]),
        1,
        1,
    )
    .unwrap()
}
#[test]
fn connected_policy_requires_root_signature_active_exact_binding_and_fresh_key() {
    let (root, authority_signer) = root();
    let bound = binding("device-1");
    let device = VesselDeviceKeypairV1::from_private_bytes(bound.key_id.clone(), [9; 32]);
    let mut authority = FleetEnrollmentAuthorityV1::default();
    let response = device
        .respond(
            authority
                .issue_challenge(bound.clone(), device.public(), 10, 20)
                .unwrap(),
        )
        .unwrap();
    let grant = authority.accept_response(response, 11, 1, 100).unwrap();
    let signed = authority_signer
        .sign(
            "fleet-policy-envelope",
            BoundPolicyEnvelopeV1 {
                binding: bound.clone(),
                policy: policy(),
            },
        )
        .unwrap();
    assert!(authority.admit_policy(&signed, &root, 12).is_ok());
    let mut cross_scope = signed.clone();
    cross_scope.payload.binding.fleet_id = FleetId::new("fleet-other").unwrap();
    assert!(authority.admit_policy(&cross_scope, &root, 12).is_err());
    authority.revoke(&grant.key.binding).unwrap();
    assert!(authority.admit_policy(&signed, &root, 12).is_err());
}

#[test]
fn signed_card_policy_registry_and_device_request_are_all_required() {
    let (root, authority_signer) = root();
    let bound = binding("device-card");
    let device = VesselDeviceKeypairV1::from_private_bytes(bound.key_id.clone(), [12; 32]);
    let mut authority = FleetEnrollmentAuthorityV1::default();
    let response = device
        .respond(
            authority
                .issue_challenge(bound.clone(), device.public(), 10, 20)
                .unwrap(),
        )
        .unwrap();
    authority.accept_response(response, 11, 1, 100).unwrap();
    let capability = "mission.execute.offline".to_owned();
    let policy = policy();
    let signed_policy = authority_signer
        .sign(
            "fleet-policy-envelope",
            BoundPolicyEnvelopeV1 {
                binding: bound.clone(),
                policy: policy.clone(),
            },
        )
        .unwrap();
    let raw_card = AgentCardV1 {
        card_id: AgentCardId::new("card-a").unwrap(),
        agent_id: AgentId::new("agent-a").unwrap(),
        revision: 1,
        vessel_id: bound.vessel_id.clone(),
        fleet_node_id: bound.fleet_node_id.clone(),
        key_id: bound.key_id.clone(),
        capabilities: BTreeSet::from([capability.clone()]),
        policy_digest: policy.policy_digest.clone(),
    };
    // Tuple admission is deliberately insufficient: connected task admission requires the two
    // Fleet signatures, immutable registry membership, and the device request signature below.
    assert!(authority.admit_card(&raw_card, &bound, 12).is_ok());
    let signed_card = authority_signer
        .sign(
            "fleet-agent-card",
            BoundAgentCardV1 {
                binding: bound.clone(),
                card: raw_card.clone(),
                provenance: FleetCardProvenanceV1 {
                    tenant_id: bound.tenant_id.clone(),
                    fleet_id: bound.fleet_id.clone(),
                    issuer_node_id: FleetNodeId::new("fleet-core").unwrap(),
                },
                issued_at_ms: 11,
                expires_at_ms: 90,
            },
        )
        .unwrap();
    let registry = A2aCapabilityRegistryV1::new(1, BTreeSet::from([capability.clone()])).unwrap();
    let request = device
        .sign(
            "fleet-a2a-task-submit",
            A2aTaskSubmissionV1 {
                binding: bound,
                card: signed_card,
                policy: signed_policy,
                registry_revision: registry.revision(),
                registry_digest: registry.digest().clone(),
                task: A2ATaskV1 {
                    task_id: TaskId::new("task-a").unwrap(),
                    card_id: raw_card.card_id,
                    card_revision: 1,
                    mission_revision: mission(&policy, "mission-a"),
                    requested_capability: capability,
                    canonical_input: br#"{"nodes":["planner","worker"]}"#.to_vec(),
                    state: A2ATaskStateV1::Submitted,
                },
            },
        )
        .unwrap();
    assert!(authority.admit_task(&request, &root, &registry, 12).is_ok());
    let mut tampered = request;
    tampered.payload.task.canonical_input = br#"{"nodes":["worker"]}"#.to_vec();
    assert_eq!(
        authority.admit_task(&tampered, &root, &registry, 12),
        Err(EnrollmentError::Verification)
    );
}

#[test]
fn connected_sync_requires_the_signing_root_scope() {
    let (root, authority_signer) = root();
    let mut cross_fleet_binding = binding("device-2");
    cross_fleet_binding.fleet_id = FleetId::new("fleet-b").unwrap();
    let device =
        VesselDeviceKeypairV1::from_private_bytes(cross_fleet_binding.key_id.clone(), [10; 32]);
    let mut authority = FleetEnrollmentAuthorityV1::default();
    let response = device
        .respond(
            authority
                .issue_challenge(cross_fleet_binding.clone(), device.public(), 10, 20)
                .unwrap(),
        )
        .unwrap();
    authority.accept_response(response, 11, 1, 100).unwrap();
    let record = FleetRecordV1::new(
        RecordHeaderV1 {
            schema_version: SCHEMA_VERSION_V1,
            record_id: RecordId::new("sync-record-1").unwrap(),
            tenant_id: cross_fleet_binding.tenant_id.clone(),
            fleet_id: cross_fleet_binding.fleet_id.clone(),
            vessel_id: Some(cross_fleet_binding.vessel_id.clone()),
            correlation_id: CorrelationId::new("sync-correlation-1").unwrap(),
            causation_id: None,
            hlc: HybridLogicalClockV1 {
                physical_ms: 12,
                logical: 0,
                node_id: cross_fleet_binding.fleet_node_id.clone(),
            },
            local_sequence: 1,
            ownership: OwnershipV1::SharedAppendOnly,
            conflict_state: ConflictStateV1::None,
            provenance: ProvenanceV1 {
                origin_node_id: cross_fleet_binding.fleet_node_id.clone(),
                principal_id: PrincipalId::new("fleet-principal").unwrap(),
                trust_domain: "fleet".into(),
                source_record_digest: None,
            },
        },
        FleetRecordPayloadV1::SyncManifest(SyncManifestV1 {
            vessel_id: cross_fleet_binding.vessel_id.clone(),
            supported_schema_versions: BTreeSet::from([SCHEMA_VERSION_V1]),
            checkpoints: BTreeMap::new(),
            max_batch_records: 1,
        }),
    )
    .unwrap();
    let signed = authority_signer
        .sign(
            "fleet-sync-envelope",
            BoundSyncEnvelopeV1 {
                binding: cross_fleet_binding,
                record,
            },
        )
        .unwrap();
    assert!(authority.admit_sync(&signed, &root, 12).is_err());
}
