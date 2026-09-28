use super::*;

const OPERATION: &str = "550e8400-e29b-41d4-a716-446655440000";

fn fixture() -> (
    ReviewRequest,
    TargetObservation,
    CandidateObservation,
    BoundaryObservation,
) {
    (
        ReviewRequest {
            schema_version: 1,
            operation_id: OPERATION.into(),
            target_path: "/Applications/Example.app".into(),
            expected_bundle_identifier: "org.example.App".into(),
            expected_installed_build: "100".into(),
            expected_candidate_build: "101".into(),
        },
        TargetObservation {
            canonical_path: "/Applications/Example.app".into(),
            device: 1,
            inode: 2,
            bundle_identifier: "org.example.App".into(),
            build: "100".into(),
            team_identifier: "ABCDE12345".into(),
            code_directory_hash: vec![1; 20],
            signature_valid: true,
            ed25519_public_key: vec![2; 32],
            feed_url: "https://example.org/updates/appcast.xml".into(),
            framework_major: 2,
            authority: Authority::Standalone,
            has_store_receipt: false,
            translocated: false,
            writable_by_others: false,
        },
        CandidateObservation {
            build: "101".into(),
            feed_url: "https://example.org/updates/appcast.xml".into(),
            archive_url: "https://cdn.example.org/Example.zip".into(),
            archive_length: 2048,
            ed25519_signature: vec![3; 64],
            channel: None,
            is_full_zip_application: true,
            sparkle_accepts_upgrade: true,
        },
        BoundaryObservation {
            helper_identifier: HELPER_IDENTIFIER.into(),
            helper_team_identifier: HELM_TEAM.into(),
            helper_code_directory_hash: vec![4; 20],
            caller_identifier: HELM_IDENTIFIER.into(),
            caller_team_identifier: HELM_TEAM.into(),
            authenticated_live_caller: true,
            developer_id_signature_valid: true,
            notarization_accepted: true,
            helm_sandbox_preserved: true,
            external_helper_unsandboxed: true,
            direct_consumer_channel: true,
        },
    )
}

fn roots() -> Vec<PathBuf> {
    vec!["/Applications".into(), "/Users/example/Applications".into()]
}

fn review() -> ReviewedUpdate {
    let (request, target, candidate, boundary) = fixture();
    ReviewedUpdate::prepare(request, target, candidate, boundary, &roots(), 100).unwrap()
}

fn session() -> UpdateSession {
    let (_, target, candidate, boundary) = fixture();
    review()
        .confirm(target, candidate, boundary, &roots(), 110)
        .unwrap()
}

#[test]
fn request_is_bounded_versioned_and_cannot_supply_commands_or_trust() {
    let (request, ..) = fixture();
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(ReviewRequest::decode(&bytes).unwrap(), request);
    for field in [
        "command",
        "environment",
        "feedURL",
        "signatureValid",
        "roots",
    ] {
        let mut value = serde_json::to_value(&request).unwrap();
        value[field] = serde_json::json!("caller controlled");
        assert_eq!(
            ReviewRequest::decode(&serde_json::to_vec(&value).unwrap()),
            Err(Rejection::MalformedRequest),
            "{field}"
        );
    }
    assert!(ReviewRequest::decode(&vec![b' '; MAX_REQUEST_BYTES + 1]).is_err());
    for id in ["", "not-a-uuid", "550e8400-e29b-41d4-a716-44665544000g"] {
        let mut invalid = request.clone();
        invalid.operation_id = id.into();
        assert!(invalid.validate().is_err());
    }
    let mut future = request;
    future.schema_version = 2;
    assert!(future.validate().is_err());
}

#[test]
fn noncanonical_nested_system_and_lookalike_roots_are_rejected() {
    for path in [
        "Applications/Example.app",
        "/Applications/../Example.app",
        "/Applications/./Example.app",
        "/Applications//Example.app",
        "/Applications/Example.app/",
        "/Applications/Host.app/Nested.app",
        "/System/Applications/Example.app",
        "/ApplicationsFake/Example.app",
        "/Volumes/Installer/Example.app",
        "/Applications/Example.app\n",
    ] {
        let (mut request, mut target, candidate, boundary) = fixture();
        request.target_path = path.into();
        target.canonical_path = path.into();
        assert!(
            ReviewedUpdate::prepare(request, target, candidate, boundary, &roots(), 0).is_err(),
            "{path}"
        );
    }
    let (mut request, mut target, candidate, boundary) = fixture();
    request.target_path = "/Users/example/Applications/Tools/Example.app".into();
    target.canonical_path = request.target_path.clone();
    assert!(ReviewedUpdate::prepare(request, target, candidate, boundary, &roots(), 0).is_ok());
}

#[test]
fn helm_variants_are_never_external_targets() {
    for identifier in [
        HELM_IDENTIFIER,
        "com.jasoncavinder.Helm.QA",
        "COM.JASONCAVINDER.HELM",
    ] {
        let (mut request, mut target, candidate, boundary) = fixture();
        request.expected_bundle_identifier = identifier.into();
        target.bundle_identifier = identifier.into();
        assert_eq!(
            ReviewedUpdate::prepare(request, target, candidate, boundary, &roots(), 0).unwrap_err(),
            Rejection::HelmSelfUpdate
        );
    }
}

#[test]
fn caller_configuration_cannot_expand_roots_to_system_apps() {
    let (mut request, mut target, candidate, boundary) = fixture();
    request.target_path = "/System/Applications/Example.app".into();
    target.canonical_path = request.target_path.clone();
    assert_eq!(
        ReviewedUpdate::prepare(
            request,
            target,
            candidate,
            boundary,
            &["/System/Applications".into()],
            0
        )
        .unwrap_err(),
        Rejection::TargetOutsideRoots
    );
}

#[test]
fn every_boundary_requirement_is_mandatory() {
    for change in 0..11 {
        let (request, target, candidate, mut boundary) = fixture();
        match change {
            0 => boundary.authenticated_live_caller = false,
            1 => boundary.developer_id_signature_valid = false,
            2 => boundary.notarization_accepted = false,
            3 => boundary.helm_sandbox_preserved = false,
            4 => boundary.external_helper_unsandboxed = false,
            5 => boundary.direct_consumer_channel = false,
            6 => boundary.helper_identifier = "org.example.Helper".into(),
            7 => boundary.helper_team_identifier = "OTHER12345".into(),
            8 => boundary.caller_identifier = "com.jasoncavinder.OtherApp".into(),
            9 => boundary.caller_team_identifier = "OTHER12345".into(),
            _ => boundary.helper_code_directory_hash.clear(),
        }
        assert_eq!(
            ReviewedUpdate::prepare(request, target, candidate, boundary, &roots(), 0).unwrap_err(),
            Rejection::BoundaryUnavailable,
            "{change}"
        );
    }
}

#[test]
fn unsupported_or_changed_native_targets_require_vendor_fallback() {
    for change in 0..14 {
        let (request, mut target, candidate, boundary) = fixture();
        match change {
            0 => target.canonical_path = "/Applications/Other.app".into(),
            1 => target.bundle_identifier = "org.other.App".into(),
            2 => target.build = "99".into(),
            3 => target.signature_valid = false,
            4 => target.team_identifier = "adhoc".into(),
            5 => target.code_directory_hash = vec![0; 20],
            6 => target.ed25519_public_key.clear(),
            7 => target.framework_major = 1,
            8 => target.authority = Authority::OtherManager,
            9 => target.authority = Authority::Unknown,
            10 => target.has_store_receipt = true,
            11 => target.translocated = true,
            12 => target.writable_by_others = true,
            _ => target.feed_url = "http://example.org/feed.xml".into(),
        }
        assert!(
            ReviewedUpdate::prepare(request, target, candidate, boundary, &roots(), 0).is_err(),
            "{change}"
        );
    }
}

#[test]
fn exact_supported_candidate_is_required_not_only_a_version_label() {
    for change in 0..11 {
        let (request, target, mut candidate, boundary) = fixture();
        match change {
            0 => candidate.build = "102".into(),
            1 => candidate.build = "100".into(),
            2 => candidate.feed_url = "https://other.example/feed".into(),
            3 => candidate.archive_url = "http://example.org/app.zip".into(),
            4 => candidate.archive_url = "https://user:password@example.org/app.zip".into(),
            5 => candidate.archive_url = "https://example.org/app.zip#fragment".into(),
            6 => candidate.archive_length = 0,
            7 => candidate.archive_length = u64::MAX,
            8 => candidate.ed25519_signature.clear(),
            9 => candidate.channel = Some("beta".into()),
            _ => candidate.is_full_zip_application = false,
        }
        assert!(
            ReviewedUpdate::prepare(request, target, candidate, boundary, &roots(), 0).is_err(),
            "{change}"
        );
    }
    let (request, target, mut candidate, boundary) = fixture();
    candidate.sparkle_accepts_upgrade = false;
    assert_eq!(
        ReviewedUpdate::prepare(request, target, candidate, boundary, &roots(), 0).unwrap_err(),
        Rejection::UnsupportedCandidate
    );
}

#[test]
fn confirmation_expires_and_binds_local_and_remote_identity() {
    for now in [99, 221, u64::MAX] {
        let (_, target, candidate, boundary) = fixture();
        assert_eq!(
            review()
                .confirm(target, candidate, boundary, &roots(), now)
                .unwrap_err(),
            Rejection::ReviewExpired
        );
    }
    for change in 0..7 {
        let (_, mut target, mut candidate, mut boundary) = fixture();
        match change {
            0 => target.inode += 1,
            1 => target.code_directory_hash[0] += 1,
            2 => target.ed25519_public_key[0] += 1,
            3 => candidate.archive_url = "https://other.example/app.zip".into(),
            4 => candidate.archive_length += 1,
            5 => candidate.ed25519_signature[0] += 1,
            _ => boundary.helper_code_directory_hash[0] += 1,
        }
        assert_eq!(
            review()
                .confirm(target, candidate, boundary, &roots(), 110)
                .unwrap_err(),
            Rejection::ReviewChanged,
            "{change}"
        );
    }
    let (_, target, candidate, boundary) = fixture();
    assert!(
        review()
            .confirm(target, candidate, boundary, &roots(), 220)
            .is_ok()
    );
}

#[test]
fn installer_success_cannot_be_reported_as_verified_version() {
    let mut session = session();
    let (_, mut installed, ..) = fixture();
    assert_eq!(
        session.reconcile(OPERATION, &installed),
        Err(Rejection::InvalidTransition)
    );
    session.event(OPERATION, UpdateEvent::Downloaded).unwrap();
    assert_eq!(session.state(), UpdateState::ReadyToInstall);
    assert_eq!(
        session.event(OPERATION, UpdateEvent::InstallerFinished),
        Err(Rejection::InvalidTransition)
    );
    session
        .event(OPERATION, UpdateEvent::InstallationWillBegin)
        .unwrap();
    session
        .event(OPERATION, UpdateEvent::InstallerFinished)
        .unwrap();
    assert_eq!(session.state(), UpdateState::AwaitingVerification);
    assert_eq!(
        session.reconcile(OPERATION, &installed).unwrap(),
        UpdateState::Unverified
    );
    installed.build = "101".into();
    installed.inode += 1;
    installed.code_directory_hash = vec![5; 20];
    assert_eq!(
        session.reconcile(OPERATION, &installed).unwrap(),
        UpdateState::VersionVerified
    );
    assert_eq!(
        session.event(OPERATION, UpdateEvent::Failed),
        Err(Rejection::InvalidTransition)
    );
}

#[test]
fn cancellation_and_loss_after_install_handoff_remain_unverified() {
    for event in [
        UpdateEvent::Cancelled,
        UpdateEvent::Failed,
        UpdateEvent::ConnectionLost,
    ] {
        let mut before = session();
        before.event(OPERATION, event).unwrap();
        assert_eq!(
            before.state(),
            if event == UpdateEvent::Cancelled {
                UpdateState::CancelledBeforeInstall
            } else {
                UpdateState::FailedBeforeInstall
            }
        );
        let mut after = session();
        after.event(OPERATION, UpdateEvent::Downloaded).unwrap();
        after
            .event(OPERATION, UpdateEvent::InstallationWillBegin)
            .unwrap();
        after.event(OPERATION, event).unwrap();
        assert_eq!(after.state(), UpdateState::Unverified);
        assert_eq!(
            after.event(OPERATION, UpdateEvent::InstallationWillBegin),
            Err(Rejection::InvalidTransition)
        );
    }
}

#[test]
fn wrong_operation_events_cannot_mutate_session_state() {
    let mut session = session();
    assert_eq!(
        session.event("different", UpdateEvent::Downloaded),
        Err(Rejection::WrongOperation)
    );
    assert_eq!(session.state(), UpdateState::Downloading);
    assert_eq!(
        session.reconcile("different", &fixture().1),
        Err(Rejection::WrongOperation)
    );
}

#[test]
fn replacement_requires_same_vendor_scope_and_exact_observed_build() {
    for change in 0..9 {
        let mut session = session();
        session.event(OPERATION, UpdateEvent::Downloaded).unwrap();
        session
            .event(OPERATION, UpdateEvent::InstallationWillBegin)
            .unwrap();
        session
            .event(OPERATION, UpdateEvent::InstallerFinished)
            .unwrap();
        let (_, mut observed, ..) = fixture();
        observed.build = "101".into();
        match change {
            0 => observed.build = "102".into(),
            1 => observed.signature_valid = false,
            2 => observed.team_identifier = "OTHER12345".into(),
            3 => observed.canonical_path = "/Applications/Other.app".into(),
            4 => observed.bundle_identifier = "org.other.App".into(),
            5 => observed.ed25519_public_key = vec![7; 32],
            6 => observed.authority = Authority::OtherManager,
            7 => observed.translocated = true,
            _ => observed.writable_by_others = true,
        }
        assert_eq!(
            session.reconcile(OPERATION, &observed).unwrap(),
            UpdateState::Unverified,
            "{change}"
        );
    }
}
