use std::path::PathBuf;
use std::time::SystemTime;

use helm_core::adapters::uv_tool::{
    UvToolListError, UvToolListMode, parse_uv_tool_list, uv_tool_list_command,
};
use helm_core::execution::{ProcessExitStatus, ProcessOutput};

const INSTALLED: &str = include_str!("fixtures/uv/installed.txt");
const LATEST: &str = include_str!("fixtures/uv/latest.txt");

fn output(stdout: &str, stderr: &str) -> ProcessOutput {
    ProcessOutput {
        status: ProcessExitStatus::ExitCode(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        started_at: SystemTime::UNIX_EPOCH,
        finished_at: SystemTime::UNIX_EPOCH,
    }
}

#[test]
fn installed_command_is_local_uncolored_and_preserves_constraints() {
    let command = uv_tool_list_command("/selected tools/uv", UvToolListMode::Installed);
    assert_eq!(command.program, PathBuf::from("/selected tools/uv"));
    assert_eq!(
        command.args,
        [
            "--color",
            "never",
            "--no-progress",
            "tool",
            "list",
            "--show-version-specifiers",
            "--offline"
        ]
    );
    assert_eq!(
        command.env.get("UV_PYTHON_DOWNLOADS").map(String::as_str),
        Some("never")
    );
    assert!(!command.env.contains_key("UV_TOOL_DIR"));
    assert!(!command.args.contains(&"--no-config".to_owned()));
}

#[test]
fn latest_command_only_discovers_versions_without_mutating_tools() {
    let command = uv_tool_list_command("uv", UvToolListMode::LatestVersions);
    assert_eq!(
        command.args,
        [
            "--color",
            "never",
            "--no-progress",
            "tool",
            "list",
            "--show-version-specifiers",
            "--outdated"
        ]
    );
    assert_eq!(
        command.env.get("UV_PYTHON_DOWNLOADS").map(String::as_str),
        Some("never")
    );
}

#[test]
fn entrypoints_remain_children_of_one_tool() {
    let tools = parse_uv_tool_list(&output(INSTALLED, ""), UvToolListMode::Installed).unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "black");
    assert_eq!(tools[0].installed_version, "24.2.0");
    assert_eq!(tools[0].requirement.as_deref(), Some("<24.3.0"));
    assert_eq!(tools[0].executables, ["black", "blackd"]);
    assert_eq!(tools[0].latest_version, None);
    assert_eq!(tools[1].name, "flask");
    assert_eq!(tools[1].requirement, None);
}

#[test]
fn discovery_preserves_an_exact_pin_alongside_an_ineligible_latest_version() {
    let tools = parse_uv_tool_list(&output(LATEST, ""), UvToolListMode::LatestVersions).unwrap();
    assert_eq!(tools[0].requirement.as_deref(), Some("==24.2.0"));
    assert_eq!(tools[0].latest_version.as_deref(), Some("24.3.0"));
    assert_eq!(tools[0].installed_version, "24.2.0");
}

#[test]
fn empty_store_requires_the_known_installed_success_contract() {
    for mode in [UvToolListMode::Installed, UvToolListMode::LatestVersions] {
        assert!(
            parse_uv_tool_list(&output("", "No tools installed\n"), mode)
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(
        parse_uv_tool_list(&output("", ""), UvToolListMode::Installed),
        Err(UvToolListError::UnconfirmedEmptyInventory)
    );
    assert!(
        parse_uv_tool_list(&output("", ""), UvToolListMode::LatestVersions)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn success_with_skipped_tools_is_never_an_authoritative_snapshot() {
    for stdout in ["", INSTALLED] {
        for stderr in [
            "warning: Ignoring malformed tool `other` (run `uv tool uninstall other` to remove)\n",
            "warning: Tool `other` environment not found\n",
            "unexpected informational output\n",
        ] {
            for mode in [UvToolListMode::Installed, UvToolListMode::LatestVersions] {
                assert_eq!(
                    parse_uv_tool_list(&output(stdout, stderr), mode),
                    Err(UvToolListError::NonAuthoritative)
                );
            }
        }
    }
    assert_eq!(
        parse_uv_tool_list(
            &output(INSTALLED, "No tools installed\n"),
            UvToolListMode::Installed
        ),
        Err(UvToolListError::NonAuthoritative)
    );
}

#[test]
fn failed_or_terminated_commands_cannot_commit_partial_stdout() {
    for status in [
        ProcessExitStatus::ExitCode(1),
        ProcessExitStatus::Terminated,
    ] {
        let mut result = output(INSTALLED, "");
        result.status = status;
        assert_eq!(
            parse_uv_tool_list(&result, UvToolListMode::Installed),
            Err(UvToolListError::ProcessFailed)
        );
    }
}

#[test]
fn invalid_encoding_and_oversized_capture_are_rejected() {
    let mut result = output(INSTALLED, "");
    result.stdout.push(0xff);
    assert_eq!(
        parse_uv_tool_list(&result, UvToolListMode::Installed),
        Err(UvToolListError::InvalidEncoding)
    );
    let mut result = output(INSTALLED, "");
    result.stderr.push(0xff);
    assert_eq!(
        parse_uv_tool_list(&result, UvToolListMode::Installed),
        Err(UvToolListError::InvalidEncoding)
    );
    result.stdout = vec![b'a'; 4 * 1024 * 1024];
    assert_eq!(
        parse_uv_tool_list(&result, UvToolListMode::Installed),
        Err(UvToolListError::OutputTooLarge)
    );
}

#[test]
fn unexpected_shapes_and_terminal_controls_fail_closed() {
    for stdout in [
        "- orphan\n",
        "unknown output\n",
        "tool v1.0\n- tool\ntrailer\n",
        "tool v1.0 [extras: x]\n- tool\n",
        "tool v1.0 (/tmp/tool)\n- tool\n",
        "tool v1.0 [required: ]\n- tool\n",
        "tool v1.0 [required: >=1] [required: <2]\n- tool\n",
        "tool v1.0 [required: >=1] [latest: 2]\n- tool\n",
        "tool v1.0\n- /tmp/tool\n",
        "tool v1.0\n- tool\n- tool\n",
        "tool v1.0\n- \n",
        "\u{1b}[1mtool v1.0\n- tool\n",
        "tool v1.0\n- to\u{8}ol\n",
        "tool v1.0\n  - tool\n",
        "--flag v1.0\n- tool\n",
    ] {
        assert!(
            parse_uv_tool_list(&output(stdout, ""), UvToolListMode::Installed).is_err(),
            "accepted {stdout:?}"
        );
    }
}

#[test]
fn every_tool_header_requires_an_executable() {
    for stdout in ["tool v1.0\n", "tool v1.0\nother v2.0\n- other\n"] {
        assert_eq!(
            parse_uv_tool_list(&output(stdout, ""), UvToolListMode::Installed),
            Err(UvToolListError::MissingExecutables { line: 1 })
        );
    }
}

#[test]
fn latest_mode_requires_one_well_formed_latest_annotation() {
    for stdout in [
        INSTALLED,
        "tool v1.0 [latest: ]\n- tool\n",
        "tool v1.0 [latest: 2] suffix\n- tool\n",
        "tool v1.0 [latest: 2] [latest: 3]\n- tool\n",
        "tool v1.0 [required: >=1 [latest: 2]]\n- tool\n",
    ] {
        assert!(
            parse_uv_tool_list(&output(stdout, ""), UvToolListMode::LatestVersions).is_err(),
            "accepted {stdout:?}"
        );
    }
    let tools = parse_uv_tool_list(
        &output("tool v1.0 [latest: 2.0]\n- tool\n", ""),
        UvToolListMode::LatestVersions,
    )
    .unwrap();
    assert_eq!(tools[0].requirement, None);
}

#[test]
fn normalized_distribution_duplicates_are_rejected_without_collapsing_them() {
    let tools = parse_uv_tool_list(
        &output("Some__Tool.Name v1.0\n- executable\n", ""),
        UvToolListMode::Installed,
    )
    .unwrap();
    assert_eq!(tools[0].name, "some-tool-name");
    assert_eq!(
        parse_uv_tool_list(
            &output("Some_Tool v1.0\n- first\nsome-tool v2.0\n- second\n", ""),
            UvToolListMode::Installed
        ),
        Err(UvToolListError::DuplicateTool { line: 3 })
    );
}

#[test]
fn shared_executable_names_do_not_collapse_distinct_distributions() {
    let tools = parse_uv_tool_list(
        &output("first v1.0\n- shared\nsecond v2.0\n- shared\n", ""),
        UvToolListMode::Installed,
    )
    .unwrap();
    assert_eq!(tools.len(), 2);
}

#[test]
fn python_versions_and_nonregistry_requirements_are_preserved_not_interpreted() {
    let tools = parse_uv_tool_list(&output("tool v1!2.0rc1.post2.dev3+local.1 [required: @ git+https://example.invalid/tool.git@main]\n- tool\n", ""), UvToolListMode::Installed).unwrap();
    assert_eq!(tools[0].installed_version, "1!2.0rc1.post2.dev3+local.1");
    assert_eq!(
        tools[0].requirement.as_deref(),
        Some("@ git+https://example.invalid/tool.git@main")
    );
}

#[test]
fn crlf_is_accepted_and_diagnostics_are_not_exposed_in_errors() {
    let tools = parse_uv_tool_list(
        &output(&INSTALLED.replace('\n', "\r\n"), ""),
        UvToolListMode::Installed,
    )
    .unwrap();
    assert_eq!(tools.len(), 2);
    let error = parse_uv_tool_list(
        &output("", "https://secret:password@example.invalid/index"),
        UvToolListMode::Installed,
    )
    .unwrap_err();
    assert_eq!(error, UvToolListError::NonAuthoritative);
    assert!(!error.to_string().contains("password"));
}
