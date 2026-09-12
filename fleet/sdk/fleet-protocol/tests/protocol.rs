use fleet_protocol::*;
use std::collections::BTreeSet;

fn digest_value(label: &str) -> DigestValue {
    digest("test", label.as_bytes()).unwrap()
}
fn policy() -> PolicyEnvelopeV1 {
    PolicyEnvelopeV1::new(
        PolicyId::new("policy-1").unwrap(),
        1,
        VesselId::new("vessel-1").unwrap(),
        10,
        1000,
        true,
        BTreeSet::from(["mission.execute.offline".into()]),
        2,
        8,
    )
    .unwrap()
}
fn graph() -> MissionRevisionV1 {
    let policy = policy();
    MissionRevisionV1::new(
        MissionId::new("mission-1").unwrap(),
        MissionRevisionId::new("revision-1").unwrap(),
        1,
        policy.policy_digest,
        vec![
            MissionNodeV1 {
                node_id: MissionNodeId::new("worker").unwrap(),
                agent_id: AgentId::new("worker-agent").unwrap(),
                prompt: "work".into(),
                dependencies: vec![MissionNodeId::new("planner").unwrap()],
                max_attempts: 2,
                effect: NodeEffectV1::PureModel,
            },
            MissionNodeV1 {
                node_id: MissionNodeId::new("planner").unwrap(),
                agent_id: AgentId::new("planner-agent").unwrap(),
                prompt: "plan".into(),
                dependencies: vec![],
                max_attempts: 2,
                effect: NodeEffectV1::PureModel,
            },
        ],
    )
    .unwrap()
}
fn header() -> RecordHeaderV1 {
    RecordHeaderV1 {
        schema_version: 1,
        record_id: RecordId::new("record-1").unwrap(),
        tenant_id: TenantId::new("tenant-1").unwrap(),
        fleet_id: FleetId::new("fleet-1").unwrap(),
        vessel_id: Some(VesselId::new("vessel-1").unwrap()),
        correlation_id: CorrelationId::new("correlation-1").unwrap(),
        causation_id: None,
        hlc: HybridLogicalClockV1 {
            physical_ms: 100,
            logical: 0,
            node_id: FleetNodeId::new("node-1").unwrap(),
        },
        local_sequence: 1,
        ownership: OwnershipV1::VesselLocal,
        conflict_state: ConflictStateV1::None,
        provenance: ProvenanceV1 {
            origin_node_id: FleetNodeId::new("node-1").unwrap(),
            principal_id: PrincipalId::new("principal-1").unwrap(),
            trust_domain: "vessel-local".into(),
            source_record_digest: None,
        },
    }
}

#[test]
fn canonical_record_rejects_digest_forgery_unknown_fields_and_duplicate_keys() {
    let record =
        FleetRecordV1::new(header(), FleetRecordPayloadV1::MissionRevision(graph())).unwrap();
    let bytes = record.canonical_bytes().unwrap();
    assert_eq!(FleetRecordV1::decode_canonical(&bytes).unwrap(), record);

    let mut forged: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    forged["digest"] = serde_json::Value::String(digest_value("forged").as_str().into());
    assert_eq!(
        FleetRecordV1::decode_canonical(&serde_json::to_vec(&forged).unwrap()),
        Err(ProtocolError::DigestMismatch)
    );

    let text = String::from_utf8(bytes).unwrap();
    let unknown = text.replacen("{\"header\":", "{\"unknown\":1,\"header\":", 1);
    assert!(FleetRecordV1::decode_canonical(unknown.as_bytes()).is_err());
    let duplicate = text.replacen("{\"header\":", "{\"header\":null,\"header\":", 1);
    assert_eq!(
        FleetRecordV1::decode_canonical(duplicate.as_bytes()),
        Err(ProtocolError::DuplicateKey)
    );
}

#[test]
fn mission_definition_is_distinct_from_a_digest_pinned_revision() {
    let definition = MissionDefinitionV1 {
        mission_id: MissionId::new("mission-definition").unwrap(),
    };
    let record = FleetRecordV1::new(
        header(),
        FleetRecordPayloadV1::MissionDefinition(definition.clone()),
    )
    .unwrap();
    assert_eq!(
        FleetRecordV1::decode_canonical(&record.canonical_bytes().unwrap()).unwrap(),
        record
    );
    let revision = MissionRevisionV1::new(
        definition.mission_id,
        MissionRevisionId::new("mission-definition-r1").unwrap(),
        1,
        digest_value("policy"),
        vec![MissionNodeV1 {
            node_id: MissionNodeId::new("node").unwrap(),
            agent_id: AgentId::new("agent").unwrap(),
            prompt: "bounded".into(),
            dependencies: vec![],
            max_attempts: 1,
            effect: NodeEffectV1::PureModel,
        }],
    )
    .unwrap();
    assert!(revision.revision_digest.as_str().starts_with("sha256:"));
}

#[test]
fn mission_topology_is_deterministic_and_cycles_are_rejected() {
    assert_eq!(
        graph()
            .deterministic_topological_order()
            .unwrap()
            .iter()
            .map(MissionNodeId::as_str)
            .collect::<Vec<_>>(),
        ["planner", "worker"]
    );
    let result = MissionRevisionV1::new(
        MissionId::new("cycle").unwrap(),
        MissionRevisionId::new("cycle-r1").unwrap(),
        1,
        digest_value("policy"),
        vec![
            MissionNodeV1 {
                node_id: MissionNodeId::new("a").unwrap(),
                agent_id: AgentId::new("agent").unwrap(),
                prompt: "a".into(),
                dependencies: vec![MissionNodeId::new("b").unwrap()],
                max_attempts: 1,
                effect: NodeEffectV1::PureModel,
            },
            MissionNodeV1 {
                node_id: MissionNodeId::new("b").unwrap(),
                agent_id: AgentId::new("agent").unwrap(),
                prompt: "b".into(),
                dependencies: vec![MissionNodeId::new("a").unwrap()],
                max_attempts: 1,
                effect: NodeEffectV1::PureModel,
            },
        ],
    );
    assert!(matches!(
        result,
        Err(ProtocolError::InvalidInvariant(
            "mission revision contains a cycle"
        ))
    ));
}

#[test]
fn hlc_merge_orders_equal_physical_time_without_wall_clock_regression() {
    let local = HybridLogicalClockV1 {
        physical_ms: 100,
        logical: 2,
        node_id: FleetNodeId::new("a").unwrap(),
    };
    let remote = HybridLogicalClockV1 {
        physical_ms: 100,
        logical: 5,
        node_id: FleetNodeId::new("b").unwrap(),
    };
    assert_eq!(local.merge(&remote, 99).unwrap().logical, 6);
    assert_eq!(local.next(50).unwrap().physical_ms, 100);
    let saturated = HybridLogicalClockV1 {
        physical_ms: 100,
        logical: u32::MAX,
        node_id: FleetNodeId::new("a").unwrap(),
    };
    assert!(matches!(
        saturated.next(100),
        Err(ProtocolError::InvalidInvariant(
            "HLC logical counter overflow"
        ))
    ));
    assert!(matches!(
        saturated.merge(&remote, 100),
        Err(ProtocolError::InvalidInvariant(
            "HLC logical counter overflow"
        ))
    ));
}

#[test]
fn sync_batch_and_officer_scope_fail_closed() {
    let batch = SyncBatchV1 {
        stream_id: StreamId::new("events").unwrap(),
        from_sequence: 1,
        to_sequence: 3,
        record_digests: vec![digest_value("one")],
        previous_batch_digest: None,
    };
    assert!(FleetRecordV1::new(header(), FleetRecordPayloadV1::SyncBatch(batch)).is_err());
    let repeated = digest_value("one");
    let duplicate = SyncBatchV1 {
        stream_id: StreamId::new("events").unwrap(),
        from_sequence: 1,
        to_sequence: 2,
        record_digests: vec![repeated.clone(), repeated],
        previous_batch_digest: None,
    };
    assert!(FleetRecordV1::new(header(), FleetRecordPayloadV1::SyncBatch(duplicate)).is_err());
    let task = A2ATaskV1 {
        task_id: TaskId::new("task").unwrap(),
        card_id: AgentCardId::new("card").unwrap(),
        card_revision: 1,
        mission_revision: graph(),
        requested_capability: "plan".into(),
        canonical_input: br#"{ "not":"canonical"}"#.to_vec(),
        state: A2ATaskStateV1::Submitted,
    };
    assert!(FleetRecordV1::new(header(), FleetRecordPayloadV1::A2ATask(task)).is_err());
    let officer = OfficerRecordV1 {
        officer_id: OfficerId::new("security").unwrap(),
        kind: OfficerKindV1::Security,
        incident_id: IncidentId::new("incident").unwrap(),
        source_record_digest: digest_value("source"),
        severity: 5,
        observation: "found".into(),
        recommendation: None,
        observe_only: false,
    };
    assert!(FleetRecordV1::new(header(), FleetRecordPayloadV1::OfficerRecord(officer)).is_err());
}
