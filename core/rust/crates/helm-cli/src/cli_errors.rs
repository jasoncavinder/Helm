use helm_core::execution::TaskOutputRecord;

pub(crate) const JSON_ERROR_EMITTED_PREFIX: &str = "__HELM_JSON_ERROR_EMITTED__:";
pub(crate) const EXIT_CODE_MARKER_PREFIX: &str = "__HELM_EXIT_CODE__:";

pub(crate) fn exit_code_for_error(error: &str) -> u8 {
    let (marked_exit_code, _) = strip_exit_code_marker(error);
    marked_exit_code.unwrap_or(1)
}

pub(crate) fn mark_json_error_emitted(error: impl AsRef<str>) -> String {
    format!("{JSON_ERROR_EMITTED_PREFIX}{}", error.as_ref())
}

pub(crate) fn strip_json_error_marker(error: &str) -> (bool, &str) {
    if let Some(stripped) = error.strip_prefix(JSON_ERROR_EMITTED_PREFIX) {
        (true, stripped)
    } else {
        (false, error)
    }
}

pub(crate) fn mark_exit_code(error: impl AsRef<str>, exit_code: u8) -> String {
    format!("{EXIT_CODE_MARKER_PREFIX}{exit_code}:{}", error.as_ref())
}

pub(crate) fn strip_exit_code_marker(error: &str) -> (Option<u8>, &str) {
    let Some(stripped) = error.strip_prefix(EXIT_CODE_MARKER_PREFIX) else {
        return (None, error);
    };
    let Some((raw_code, remainder)) = stripped.split_once(':') else {
        return (None, error);
    };
    match raw_code.parse::<u8>() {
        Ok(code) => (Some(code), remainder),
        Err(_) => (None, error),
    }
}

pub(crate) fn classify_failure_class(
    output: Option<&TaskOutputRecord>,
    message: Option<&str>,
) -> &'static str {
    if let Some(output) = output {
        if let Some(code) = output.error_code.as_deref() {
            match code {
                "hard_timeout" => return "hard_timeout",
                "idle_timeout" => return "idle_timeout",
                _ => {}
            }
        }
        if matches!(output.termination_reason.as_deref(), Some("timeout")) {
            return "timeout";
        }
        if let Some(code) = output.error_code.as_deref() {
            match code {
                "timeout" => return "timeout",
                "unsupported_capability" => return "unsupported_capability",
                _ => {}
            }
        }
    }

    let normalized = message.unwrap_or_default().to_ascii_lowercase();
    if normalized.contains("timed out waiting for coordinator response")
        || normalized.contains("coordinator response")
    {
        return "coordinator_timeout";
    }
    if normalized.contains("unsupported capability") {
        return "unsupported_capability";
    }
    if normalized.contains("current working directory must exist")
        || normalized.contains("process.cwd failed")
        || normalized.contains("could not locate working directory")
        || normalized.contains("getcwd")
    {
        return "cwd_missing";
    }
    if normalized.contains("captive portal")
        || normalized.contains("network authentication required")
        || normalized.contains("http 511")
        || normalized.contains("wifi login")
        || normalized.contains("sign in to network")
        || normalized.contains("captive")
    {
        return "network_captive_portal";
    }
    if normalized.contains("proxy authentication required")
        || normalized.contains("http 407")
        || normalized.contains("proxyconnect")
        || normalized.contains("via proxy")
        || normalized.contains("proxy error")
        || normalized.contains("https_proxy")
        || normalized.contains("http_proxy")
        || normalized.contains("all_proxy")
        || normalized.contains(" proxy ")
    {
        return "network_proxy";
    }
    if normalized.contains("temporary failure in name resolution")
        || normalized.contains("name or service not known")
        || normalized.contains("failed to lookup address")
        || normalized.contains("could not resolve host")
        || normalized.contains("dns")
    {
        return "network_dns";
    }
    if normalized.contains("network is unreachable")
        || normalized.contains("no route to host")
        || normalized.contains("not connected to the internet")
        || normalized.contains("check your internet connection")
        || normalized.contains("offline")
        || normalized.contains("failed to connect")
        || normalized.contains("connection refused")
    {
        return "network_offline";
    }
    if normalized.contains("timed out") || normalized.contains("timeout") {
        return "timeout";
    }
    if normalized.contains("no output") {
        return "idle_timeout";
    }
    "other"
}

pub(crate) fn failure_class_hint(code: &str) -> Option<&'static str> {
    match code {
        "network_dns" => Some("Check DNS resolution and retry the operation."),
        "network_offline" => Some("Check internet connectivity and retry the operation."),
        "network_proxy" => Some("Check proxy configuration and credentials, then retry."),
        "network_captive_portal" => {
            Some("Complete captive-portal sign-in in a browser, then retry.")
        }
        "timeout" | "hard_timeout" | "idle_timeout" => Some(
            "Retry, or increase the manager timeout profile if this operation is expected to run longer.",
        ),
        "coordinator_timeout" => {
            Some("Run 'helm diagnostics summary' to inspect coordinator health, then retry.")
        }
        "cwd_missing" => Some("Run Helm from an existing working directory and retry."),
        _ => None,
    }
}

pub(crate) fn failure_class_hint_string(code: &str) -> Option<String> {
    failure_class_hint(code).map(str::to_string)
}
