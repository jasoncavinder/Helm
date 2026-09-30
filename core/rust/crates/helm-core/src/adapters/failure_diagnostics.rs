use crate::models::ManagerId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessFailureDiagnostic {
    CargoToolchainUnavailable,
    CargoOfflineCacheMiss,
    DnsResolutionFailed,
    EndpointUnreachable,
    CargoBuildFailed,
    CargoReceiptUnsupported,
    CargoPublishedLockUnavailable,
}

impl ProcessFailureDiagnostic {
    pub(crate) fn marker(self) -> &'static str {
        match self {
            Self::CargoToolchainUnavailable => "cargo_toolchain_unavailable",
            Self::CargoOfflineCacheMiss => "cargo_offline_cache_miss",
            Self::DnsResolutionFailed => "dns_resolution_failed",
            Self::EndpointUnreachable => "endpoint_unreachable",
            Self::CargoBuildFailed => "cargo_build_failed",
            Self::CargoReceiptUnsupported => "cargo_receipt_unsupported",
            Self::CargoPublishedLockUnavailable => "cargo_published_lock_unavailable",
        }
    }

    pub(crate) fn issue_key(self) -> &'static str {
        match self {
            Self::CargoToolchainUnavailable => "cargo.toolchain_unavailable",
            Self::CargoOfflineCacheMiss => "cargo.offline_cache_miss",
            Self::DnsResolutionFailed => "network.dns_resolution_failed",
            Self::EndpointUnreachable => "network.endpoint_unreachable",
            Self::CargoBuildFailed => "cargo.build_failed",
            Self::CargoReceiptUnsupported => "cargo.receipt_unsupported",
            Self::CargoPublishedLockUnavailable => "cargo.published_lock_unavailable",
        }
    }

    pub(crate) fn owner(self) -> &'static str {
        match self {
            Self::CargoToolchainUnavailable
            | Self::CargoOfflineCacheMiss
            | Self::CargoReceiptUnsupported => "local_configuration",
            Self::CargoBuildFailed | Self::CargoPublishedLockUnavailable => "package_build",
            Self::DnsResolutionFailed | Self::EndpointUnreachable => "undetermined",
        }
    }

    pub(crate) fn guidance(self) -> &'static str {
        match self {
            Self::CargoToolchainUnavailable => {
                "The selected Rust toolchain cannot provide Cargo. Inspect it with rustup show active-toolchain and rustup component list --installed. Review any repair before changing components or toolchains; Helm has not switched Cargo or cleared cached packages."
            }
            Self::CargoOfflineCacheMiss => {
                "Cargo's offline setting prevented a required download. Review CARGO_NET_OFFLINE and Cargo's net.offline configuration before retrying. Helm has not enabled network access or changed dependencies; this does not establish that the Mac is offline."
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
            Self::CargoReceiptUnsupported => {
                "Helm could not safely preserve or verify this Cargo installation's source and build options. Inspect the installation receipt and task details before trying again. Git, path, private registries and unsupported metadata require manual handling."
            }
            Self::CargoPublishedLockUnavailable => {
                "Helm could not verify a published lockfile for this exact Cargo upgrade. Review the package and task details. Missing, stale or unsupported lockfiles require manual handling; Helm has not retried with unlocked dependencies or another installer."
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
        if text.contains("[cargo_published_lock_unavailable]") {
            return Some(ProcessFailureDiagnostic::CargoPublishedLockUnavailable);
        }
        if text.contains("[cargo_receipt_unsupported]") {
            return Some(ProcessFailureDiagnostic::CargoReceiptUnsupported);
        }
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
        if text.contains("attempting to make an http request, but --offline was specified")
            || (text.contains("as a reminder, you're using offline mode (--offline)")
                && text.contains("no matching package named"))
        {
            return Some(ProcessFailureDiagnostic::CargoOfflineCacheMiss);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_cache_miss_is_configuration_not_compilation_or_global_connectivity() {
        let text = "error: failed to compile `cargo-binstall v1.24.0`\nCaused by:\n  failed to download `adler2 v2.0.1`\nCaused by:\n  attempting to make an HTTP request, but --offline was specified";
        let diagnostic = classify_process_failure(ManagerId::Cargo, text).unwrap();
        assert_eq!(diagnostic, ProcessFailureDiagnostic::CargoOfflineCacheMiss);
        assert_eq!(diagnostic.owner(), "local_configuration");
        assert_eq!(diagnostic.issue_key(), "cargo.offline_cache_miss");
        assert!(diagnostic.guidance().contains("CARGO_NET_OFFLINE"));
        assert_eq!(classify_process_failure(ManagerId::Npm, text), None);
        assert_eq!(
            classify_process_failure(ManagerId::Cargo, "offline package failed to compile"),
            None
        );
        assert_eq!(
            classify_process_failure(
                ManagerId::Cargo,
                "error[E0433]: missing symbol\nerror: could not compile `offline`"
            ),
            Some(ProcessFailureDiagnostic::CargoBuildFailed)
        );
    }
}
