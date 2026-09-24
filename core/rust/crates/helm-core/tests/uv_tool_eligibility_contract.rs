use helm_core::adapters::uv_tool::UvToolObservation;
use helm_core::adapters::uv_tool_eligibility::{
    UvCandidateEligibility as Eligibility, UvCandidateRejection as Rejection,
    UvEligibilityError as Error, UvRegistryCandidate, UvToolEligibilityPolicy,
};

fn installed(version: &str) -> UvToolObservation {
    UvToolObservation {
        name: "example-tool".into(),
        installed_version: version.into(),
        requirement: None,
        executables: vec!["example".into(), "example-alt".into()],
        latest_version: None,
    }
}

fn receipt(requirements: &str, rest: &str) -> String {
    format!(
        "[tool]\nrequirements = [{requirements}]\n\
         entrypoints = [\n\
         {{ name = 'example', install-path = '/isolated/bin/example', from = 'example-tool' }},\n\
         {{ name = 'example-alt', install-path = '/isolated/bin/example-alt' }},\n\
         ]\n{rest}\n"
    )
}

fn policy(version: &str, specifier: &str, rest: &str) -> UvToolEligibilityPolicy {
    UvToolEligibilityPolicy::from_receipt(
        &installed(version),
        receipt(
            &format!("{{ name = 'Example_Tool', specifier = '{specifier}' }}"),
            rest,
        )
        .as_bytes(),
    )
    .unwrap()
}

fn candidate(version: &str) -> UvRegistryCandidate {
    UvRegistryCandidate {
        name: "EXAMPLE.tool".into(),
        version: version.into(),
        requires_python: Some(">=3.9".into()),
        yanked: Some(false),
    }
}

fn check(policy: &UvToolEligibilityPolicy, version: &str) -> Eligibility {
    policy.assess(&candidate(version), Some("3.12.9")).unwrap()
}

#[test]
fn range_and_exclusions_admit_an_intermediate_update_not_latest() {
    let policy = policy("1.0", ">=1,<2,!=1.5", "");
    assert_eq!(
        check(&policy, "2.0"),
        Eligibility::Rejected(Rejection::OutsideConstraints)
    );
    assert_eq!(
        check(&policy, "1.5"),
        Eligibility::Rejected(Rejection::OutsideConstraints)
    );
    assert_eq!(check(&policy, "1.9"), Eligibility::NeedsResolution);
}

#[test]
fn exact_pin_does_not_advertise_newer_release_or_unchanged_update() {
    let policy = policy("1.0", "==1.0", "");
    assert_eq!(
        check(&policy, "2.0"),
        Eligibility::Rejected(Rejection::OutsideConstraints)
    );
    assert_eq!(
        check(&policy, "1.0.0"),
        Eligibility::Rejected(Rejection::NotNewer)
    );
}

#[test]
fn wildcard_and_compatible_release_are_python_not_semver_constraints() {
    for specifier in ["==1.4.*", "~=1.4.0"] {
        let policy = policy("1.4.0", specifier, "");
        assert_eq!(check(&policy, "1.4.9"), Eligibility::NeedsResolution);
        assert_eq!(
            check(&policy, "1.5"),
            Eligibility::Rejected(Rejection::OutsideConstraints)
        );
    }
}

#[test]
fn exclusive_prerelease_bounds_preserve_matching_candidates_for_resolution() {
    for (specifier, installed, newer) in [
        ("<2.0rc3", "2.0rc1", "2.0rc2"),
        ("<2.0.dev3", "2.0.dev1", "2.0.dev2"),
    ] {
        for (options, expected) in [
            (
                "[tool.options]\nprerelease = 'allow'",
                Eligibility::NeedsResolution,
            ),
            ("", Eligibility::NeedsPrereleaseResolution),
        ] {
            assert_eq!(
                check(&policy(installed, specifier, options), newer),
                expected,
                "{specifier}: {installed} -> {newer} ({options})"
            );
        }
    }
}

#[test]
fn exclusive_upper_bounds_respect_epochs_and_postrelease_boundaries() {
    for (specifier, newer, expected) in [
        ("<1!2.0", "2.0rc2", Eligibility::NeedsResolution),
        ("<2.0.post1", "2.0rc2", Eligibility::NeedsResolution),
        ("<2.0.post1", "2.0.post0.dev1", Eligibility::NeedsResolution),
        (
            "<2.0",
            "2.0rc2",
            Eligibility::Rejected(Rejection::OutsideConstraints),
        ),
        (
            "<2.0.post1",
            "2.0.post1.dev0",
            Eligibility::Rejected(Rejection::OutsideConstraints),
        ),
        (
            "<2.0rc3",
            "2.0rc3",
            Eligibility::Rejected(Rejection::OutsideConstraints),
        ),
    ] {
        assert_eq!(
            check(
                &policy("0.5", specifier, "[tool.options]\nprerelease = 'allow'"),
                newer
            ),
            expected,
            "{specifier}: {newer}"
        );
    }
}

#[test]
fn wildcard_matching_zero_pads_equivalent_short_versions() {
    for version in ["1", "1.0", "1.0.0"] {
        for (specifier, expected) in [
            ("!=1.4.*", Eligibility::NeedsResolution),
            ("==1.0.*", Eligibility::NeedsResolution),
            (
                "==1.4.*",
                Eligibility::Rejected(Rejection::OutsideConstraints),
            ),
            (
                "!=1.0.*",
                Eligibility::Rejected(Rejection::OutsideConstraints),
            ),
        ] {
            assert_eq!(
                check(&policy("0.5", specifier, ""), version),
                expected,
                "{specifier}: {version}"
            );
        }
    }
}

#[test]
fn requires_python_wildcards_zero_pad_the_interpreter_release() {
    let policy = policy("1.0", "", "");
    for (specifier, expected) in [
        ("!=3.12.9.1.*", Eligibility::NeedsResolution),
        ("==3.12.9.0.*", Eligibility::NeedsResolution),
        (
            "==3.12.9.1.*",
            Eligibility::Rejected(Rejection::PythonIncompatible),
        ),
        (
            "!=3.12.9.0.*",
            Eligibility::Rejected(Rejection::PythonIncompatible),
        ),
    ] {
        let mut release = candidate("2.0");
        release.requires_python = Some(specifier.into());
        assert_eq!(
            policy.assess(&release, Some("3.12.9")).unwrap(),
            expected,
            "{specifier}"
        );
    }
}

#[test]
fn python_versions_use_numeric_epoch_post_dev_and_local_ordering() {
    for (installed, newer) in [
        ("1.9", "1.10"),
        ("99.0", "1!1.0"),
        ("1.0rc1", "1.0"),
        ("1.0", "1.0.post1"),
        ("1.0.dev1", "1.0.dev2"),
        ("1.0+local.9", "1.0+local.10"),
    ] {
        let policy = policy(installed, "", "[tool.options]\nprerelease = 'allow'");
        assert_eq!(
            check(&policy, newer),
            Eligibility::NeedsResolution,
            "{installed} -> {newer}"
        );
        assert_eq!(
            check(&policy, installed),
            Eligibility::Rejected(Rejection::NotNewer)
        );
    }
}

#[test]
fn public_exact_pin_matches_local_version_per_pep440_not_string_equality() {
    assert_eq!(
        check(&policy("1.0", "==1.0", ""), "1.0+build.2"),
        Eligibility::NeedsResolution
    );
    assert_eq!(
        check(&policy("1.0+build.1", "==1.0+build.1", ""), "1.0+build.2"),
        Eligibility::Rejected(Rejection::OutsideConstraints)
    );
}

#[test]
fn additional_primary_requirements_and_constraints_are_intersected() {
    let raw = receipt(
        "{ name = 'example-tool', specifier = '>=1,<3', extras = ['speed'] },\
         { name = 'dependency', specifier = '>=2' },\
         { name = 'example_tool', specifier = '<2' }",
        "constraints = [{ name = 'example-tool', specifier = '!=1.9' }, { name = 'dependency', specifier = '<4' }]",
    );
    let policy = UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap();
    assert_eq!(check(&policy, "1.8"), Eligibility::NeedsResolution);
    for version in ["1.9", "2.0"] {
        assert_eq!(
            check(&policy, version),
            Eligibility::Rejected(Rejection::OutsideConstraints)
        );
    }
}

#[test]
fn python_requirement_gates_candidate_using_observed_environment_not_receipt_request() {
    let policy = policy("1.0", "", "python = '3.13'");
    let mut release = candidate("2.0");
    release.requires_python = Some(">=3.13,!=3.13.1".into());
    assert_eq!(
        policy.assess(&release, Some("3.12.9")).unwrap(),
        Eligibility::Rejected(Rejection::PythonIncompatible)
    );
    assert_eq!(
        policy.assess(&release, Some("3.13.1")).unwrap(),
        Eligibility::Rejected(Rejection::PythonIncompatible)
    );
    assert_eq!(
        policy.assess(&release, Some("3.13.2")).unwrap(),
        Eligibility::NeedsResolution
    );
    for python in [
        None,
        Some("3.13"),
        Some("3.13.0rc1"),
        Some("1!3.13.0"),
        Some("3.13.0+vendor"),
    ] {
        assert_eq!(
            policy.assess(&release, python).unwrap(),
            Eligibility::NeedsPythonEvidence
        );
    }
}

#[test]
fn unknown_metadata_is_not_unrestricted_metadata() {
    let policy = policy("1.0", "", "");
    let mut release = candidate("2.0");
    release.requires_python = None;
    assert_eq!(
        policy.assess(&release, Some("3.12.9")).unwrap(),
        Eligibility::NeedsDistributionMetadata
    );
    release.requires_python = Some("".into());
    release.yanked = None;
    assert_eq!(
        policy.assess(&release, Some("3.12.9")).unwrap(),
        Eligibility::NeedsDistributionMetadata
    );
    release.yanked = Some(false);
    assert_eq!(
        policy.assess(&release, Some("3.12.9")).unwrap(),
        Eligibility::NeedsResolution
    );
}

#[test]
fn prerelease_preferences_remain_resolver_owned_unless_explicitly_allowed_or_denied() {
    for mode in ["explicit", "if-necessary", "if-necessary-or-explicit"] {
        let policy = policy(
            "1.0",
            ">=1.1rc1",
            &format!("[tool.options]\nprerelease = '{mode}'"),
        );
        assert_eq!(
            check(&policy, "1.1rc2"),
            Eligibility::NeedsPrereleaseResolution
        );
        assert_eq!(check(&policy, "1.1"), Eligibility::NeedsResolution);
    }
    let default = policy("1.0", "!=1.1rc1", "");
    assert_eq!(
        check(&default, "2.0.dev1"),
        Eligibility::NeedsPrereleaseResolution
    );
    assert_eq!(
        check(
            &policy("1.0", "", "[tool.options]\nprerelease = 'allow'"),
            "2.0.dev1"
        ),
        Eligibility::NeedsResolution
    );
    assert_eq!(
        check(
            &policy("1.0", "", "[tool.options]\nprerelease = 'disallow'"),
            "2.0.dev1"
        ),
        Eligibility::Rejected(Rejection::PrereleaseDisallowed)
    );
}

#[test]
fn yanked_candidate_is_not_an_update_target() {
    let policy = policy("1.0", "", "");
    let mut release = candidate("2.0");
    release.yanked = Some(true);
    assert_eq!(
        policy.assess(&release, Some("3.12.9")).unwrap(),
        Eligibility::Rejected(Rejection::Yanked)
    );
}

#[test]
fn receipt_not_display_annotation_controls_eligibility() {
    let mut observation = installed("1.0");
    observation.requirement = Some("<99".into());
    observation.latest_version = Some("99.0".into());
    let raw = receipt("{ name = 'example-tool', specifier = '<2' }", "");
    let policy = UvToolEligibilityPolicy::from_receipt(&observation, raw.as_bytes()).unwrap();
    assert_eq!(
        check(&policy, "99.0"),
        Eligibility::Rejected(Rejection::OutsideConstraints)
    );
    assert_eq!(check(&policy, "1.1"), Eligibility::NeedsResolution);
}

#[test]
fn wrong_tool_receipt_candidate_or_entrypoints_are_rejected() {
    for raw in [
        receipt("{ name = 'another-tool' }", ""),
        receipt("{ name = 'example-tool' }", "").replace("name = 'example-alt'", "name = 'other'"),
    ] {
        assert_eq!(
            UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
            Error::IdentityMismatch
        );
    }
    let mut release = candidate("2.0");
    release.name = "another-tool".into();
    assert_eq!(
        policy("1.0", "", "")
            .assess(&release, Some("3.12.9"))
            .unwrap_err(),
        Error::IdentityMismatch
    );
}

#[test]
fn duplicate_or_empty_entrypoints_do_not_form_valid_evidence() {
    let raw = receipt("{ name = 'example-tool' }", "")
        .replace("name = 'example-alt'", "name = 'example'");
    assert_eq!(
        UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
        Error::UnsupportedReceipt
    );
    let raw = "[tool]\nrequirements = [{name = 'example-tool'}]\nentrypoints = []";
    let mut observation = installed("1.0");
    observation.executables.clear();
    assert_eq!(
        UvToolEligibilityPolicy::from_receipt(&observation, raw.as_bytes()).unwrap_err(),
        Error::IdentityMismatch
    );
}

#[test]
fn non_registry_sources_in_any_requirement_are_never_replaced_with_registry() {
    for source in ["git", "url", "path", "directory", "editable", "virtual"] {
        for requirement in [
            format!(
                "{{ name = 'example-tool', {source} = 'https://secret:token@private.invalid/x' }}"
            ),
            format!(
                "{{ name = 'example-tool' }}, {{ name = 'dependency', {source} = '/private/path' }}"
            ),
        ] {
            let raw = receipt(&requirement, "");
            let error = UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes())
                .unwrap_err();
            assert_eq!(error, Error::NonRegistrySource);
            assert!(!format!("{error:?} {error}").contains("secret"));
        }
    }
}

#[test]
fn private_indexes_and_find_links_require_source_aware_resolution() {
    for raw in [
        receipt(
            "{ name = 'example-tool', index = 'https://secret:token@private.invalid/simple' }",
            "",
        ),
        receipt(
            "{ name = 'example-tool' }",
            "[tool.options]\nfind-links = ['/private/wheels']\nno-index = true",
        ),
        receipt(
            "{ name = 'example-tool' }",
            "[tool.options]\nindex-url = 'https://secret:token@private.invalid/simple'",
        ),
    ] {
        assert_eq!(
            UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
            Error::SourceConfigurationRequiresResolution
        );
    }
}

#[test]
fn markers_groups_and_legacy_strings_are_explicit_limitations() {
    for field in ["marker = 'python_version < \"3.12\"'", "groups = ['dev']"] {
        let raw = receipt(&format!("{{ name = 'example-tool', {field} }}"), "");
        assert_eq!(
            UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
            Error::ConditionalRequirement
        );
    }
    let raw = receipt("'example-tool>=1'", "");
    assert_eq!(
        UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
        Error::LegacyRequirement
    );
}

#[test]
fn unknown_receipt_fields_and_install_policies_are_not_silently_dropped() {
    for rest in [
        "overrides = [{name = 'example-tool', specifier = '==9'}]",
        "excludes = ['dependency']",
        "build-constraint-dependencies = ['setuptools<80']",
        "surprise = true",
        "[tool.options]\nexclude-newer = '2026-01-01'",
        "[tool.options]\nresolution = 'lowest'",
        "[tool.options]\nprerelease = 'future-mode'",
        "[unknown]\nvalue = true",
    ] {
        let raw = receipt("{ name = 'example-tool' }", rest);
        assert_eq!(
            UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
            Error::UnsupportedReceipt,
            "{rest}"
        );
    }
    let raw = receipt("{ name = 'example-tool', surprise = true }", "");
    assert_eq!(
        UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
        Error::UnsupportedReceipt
    );
}

#[test]
fn malformed_input_is_sanitized_and_size_bounded() {
    for raw in [
        b"[tool secret:token".as_slice(),
        b"\xff",
        b"[tool]\nrequirements = []\nentrypoints = []",
    ] {
        let error = UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw).unwrap_err();
        assert_eq!(error, Error::UnsupportedReceipt);
        assert!(!format!("{error:?} {error}").contains("token"));
    }
    assert_eq!(
        UvToolEligibilityPolicy::from_receipt(&installed("1.0"), &vec![b' '; 1024 * 1024 + 1])
            .unwrap_err(),
        Error::InputTooLarge
    );
    let raw = receipt(&vec!["{name = 'example-tool'}"; 257].join(","), "");
    assert_eq!(
        UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
        Error::InputTooLarge
    );
    let raw = receipt(
        &format!(
            "{{name = 'example-tool', specifier = '{}'}}",
            " ".repeat(4097)
        ),
        "",
    );
    assert_eq!(
        UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
        Error::InputTooLarge
    );
}

#[test]
fn invalid_pep440_and_python_metadata_never_fall_back_to_lexical_comparison() {
    let raw = receipt("{name = 'example-tool', specifier = '^1.0'}", "");
    assert_eq!(
        UvToolEligibilityPolicy::from_receipt(&installed("1.0"), raw.as_bytes()).unwrap_err(),
        Error::InvalidVersion
    );
    let policy = policy("1.0", "", "");
    assert_eq!(
        policy
            .assess(&candidate("bananas"), Some("3.12.9"))
            .unwrap_err(),
        Error::InvalidVersion
    );
    let mut release = candidate("2.0");
    release.requires_python = Some("https://secret:token@private.invalid".into());
    let error = policy.assess(&release, Some("3.12.9")).unwrap_err();
    assert_eq!(error, Error::InvalidVersion);
    assert!(!format!("{error:?} {error}").contains("secret"));
}

#[test]
fn evidence_digest_changes_and_debug_output_omits_receipt_paths() {
    let first = policy("1.0", "<2", "python = '/private/interpreter'");
    let second = policy("1.0", "<3", "python = '/private/interpreter'");
    assert_eq!(first.receipt_digest().len(), 64);
    assert_ne!(first.receipt_digest(), second.receipt_digest());
    let debug = format!("{first:?}");
    assert!(!debug.contains("/private/interpreter"));
    assert!(!debug.contains("/isolated/bin"));
}
