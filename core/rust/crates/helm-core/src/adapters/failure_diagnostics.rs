use crate::models::ManagerId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessFailureDiagnostic {
    CargoToolchainUnavailable,
    DnsResolutionFailed,
    EndpointUnreachable,
    CargoBuildFailed,
}

impl ProcessFailureDiagnostic {
    pub(crate) fn marker(self) -> &'static str {
        match self {
            Self::CargoToolchainUnavailable => "cargo_toolchain_unavailable",
            Self::DnsResolutionFailed => "dns_resolution_failed",
            Self::EndpointUnreachable => "endpoint_unreachable",
            Self::CargoBuildFailed => "cargo_build_failed",
        }
    }

    pub(crate) fn issue_key(self) -> &'static str {
        match self {
            Self::CargoToolchainUnavailable => "cargo.toolchain_unavailable",
            Self::DnsResolutionFailed => "network.dns_resolution_failed",
            Self::EndpointUnreachable => "network.endpoint_unreachable",
            Self::CargoBuildFailed => "cargo.build_failed",
        }
    }

    pub(crate) fn owner(self) -> &'static str {
        match self {
            Self::CargoToolchainUnavailable => "local_configuration",
            Self::CargoBuildFailed => "package_build",
            Self::DnsResolutionFailed | Self::EndpointUnreachable => "undetermined",
        }
    }

    pub(crate) fn guidance(self) -> &'static str {
        match self {
            Self::CargoToolchainUnavailable => {
                "The selected Rust toolchain cannot provide Cargo. Inspect it with rustup show active-toolchain and rustup component list --installed. Review any repair before changing components or toolchains; Helm has not switched Cargo or cleared cached packages."
            }
            Self::DnsResolutionFailed => {
                "The request could not resolve its destination's network address. Check DNS, VPN, proxy, or source availability, then retry. This does not mean the entire Mac is offline; cached data remains available."
            }
            Self::EndpointUnreachable => {
                "The request could not connect to its destination. Check the source, connection, VPN, or proxy, then retry. This does not mean the entire Mac is offline; cached data remains available."
            }
            Self::CargoBuildFailed => {
                "Cargo could not compile this package or one of its dependencies. Review the compiler error and the package's supported toolchain and dependency requirements. Helm has not retried with different dependencies or another installer."
            }
        }
    }

    pub(crate) fn probes(self) -> &'static [&'static str] {
        match self {
            Self::CargoToolchainUnavailable => &[
                "rustup show active-toolchain",
                "rustup component list --installed",
                "helm tasks output <task-id>",
            ],
            _ => &[
                "helm tasks logs <task-id> --limit 250",
                "helm tasks output <task-id>",
            ],
        }
    }
}

pub(crate) fn classify_process_failure(
    manager: ManagerId,
    text: &str,
) -> Option<ProcessFailureDiagnostic> {
    let text = text.to_ascii_lowercase();
    if manager == ManagerId::Cargo {
        let unavailable = text.contains("the 'cargo' binary")
            && text.contains("'cargo' component")
            && text.contains("is not applicable")
            && text.contains("toolchain");
        let missing = (text.contains("'cargo' is not installed")
            || text.contains("'cargo' is not available"))
            && text.contains("toolchain");
        if unavailable || missing {
            return Some(ProcessFailureDiagnostic::CargoToolchainUnavailable);
        }
    }
    if [
        "temporary failure in name resolution",
        "name or service not known",
        "failed to lookup address",
        "could not resolve host",
        "could not resolve proxy",
        "getaddrinfo enotfound",
        "getaddrinfo eai_again",
    ]
    .iter()
    .any(|signature| text.contains(signature))
    {
        return Some(ProcessFailureDiagnostic::DnsResolutionFailed);
    }
    if text.contains("network is unreachable")
        || ((text.contains("https://") || text.contains("http://"))
            && (text.contains("connection refused")
                || text.contains("failed to connect")
                || text.contains("connection timed out")))
    {
        return Some(ProcessFailureDiagnostic::EndpointUnreachable);
    }
    if manager == ManagerId::Cargo
        && (text.contains("error: could not compile ")
            || text.contains("error: failed to compile "))
    {
        return Some(ProcessFailureDiagnostic::CargoBuildFailed);
    }
    None
}

/// Prefer the causal line over download/compile chatter without discarding the
/// original process log. Callers apply their usual size limit to the result.
pub(crate) fn actionable_failure_text(manager: ManagerId, text: &str) -> &str {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if (manager == ManagerId::Cargo && trimmed.starts_with("error[E"))
            || classify_process_failure(manager, line).is_some()
        {
            return &text[offset..];
        }
        offset += line.len();
    }
    text
}
