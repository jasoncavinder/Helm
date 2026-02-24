use std::path::{Path, PathBuf};

use crate::models::ManagerId;

pub const RUBYGEMS_SYSTEM_UNMANAGED_REASON_CODE: &str = "rubygems.system_unmanaged";
pub const RUBYGEMS_SYSTEM_UNMANAGED_SERVICE_ERROR_KEY: &str =
    "service.error.rubygems_system_unmanaged";
pub const RUBYGEMS_SYSTEM_UNMANAGED_MESSAGE: &str = "RubyGems at '/usr/bin/gem' is the macOS base-system installation and is not supported for Helm-managed actions. Select a non-system Ruby/Gems executable (for example Homebrew, mise, or asdf).";
pub const BUNDLER_SYSTEM_UNMANAGED_REASON_CODE: &str = "bundler.system_unmanaged";
pub const BUNDLER_SYSTEM_UNMANAGED_SERVICE_ERROR_KEY: &str =
    "service.error.bundler_system_unmanaged";
pub const BUNDLER_SYSTEM_UNMANAGED_MESSAGE: &str = "Bundler at '/usr/bin/bundle' is the macOS base-system installation and is not supported for Helm-managed actions. Select a non-system Ruby/Bundler executable (for example Homebrew, mise, or asdf).";
pub const PIP_SYSTEM_UNMANAGED_REASON_CODE: &str = "pip.system_unmanaged";
pub const PIP_SYSTEM_UNMANAGED_SERVICE_ERROR_KEY: &str = "service.error.pip_system_unmanaged";
pub const PIP_SYSTEM_UNMANAGED_MESSAGE: &str = "pip at a macOS base-system executable ('/usr/bin/python3', '/usr/bin/pip', or '/usr/bin/pip3') is not supported for Helm-managed actions. Select a non-system Python/pip executable (for example Homebrew, mise, or asdf).";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerEnablementEligibility {
    pub is_eligible: bool,
    pub reason_code: Option<&'static str>,
    pub reason_message: Option<&'static str>,
    pub service_error_key: Option<&'static str>,
}

impl ManagerEnablementEligibility {
    pub fn eligible() -> Self {
        Self {
            is_eligible: true,
            reason_code: None,
            reason_message: None,
            service_error_key: None,
        }
    }

    pub fn blocked(
        reason_code: &'static str,
        reason_message: &'static str,
        service_error_key: &'static str,
    ) -> Self {
        Self {
            is_eligible: false,
            reason_code: Some(reason_code),
            reason_message: Some(reason_message),
            service_error_key: Some(service_error_key),
        }
    }
}

pub fn manager_enablement_eligibility(
    manager: ManagerId,
    executable_path: Option<&Path>,
) -> ManagerEnablementEligibility {
    if manager == ManagerId::RubyGems && is_macos_system_rubygems_path_opt(executable_path) {
        return ManagerEnablementEligibility::blocked(
            RUBYGEMS_SYSTEM_UNMANAGED_REASON_CODE,
            RUBYGEMS_SYSTEM_UNMANAGED_MESSAGE,
            RUBYGEMS_SYSTEM_UNMANAGED_SERVICE_ERROR_KEY,
        );
    }
    if manager == ManagerId::Bundler && is_macos_system_bundler_path_opt(executable_path) {
        return ManagerEnablementEligibility::blocked(
            BUNDLER_SYSTEM_UNMANAGED_REASON_CODE,
            BUNDLER_SYSTEM_UNMANAGED_MESSAGE,
            BUNDLER_SYSTEM_UNMANAGED_SERVICE_ERROR_KEY,
        );
    }
    if manager == ManagerId::Pip && is_macos_system_pip_path_opt(executable_path) {
        return ManagerEnablementEligibility::blocked(
            PIP_SYSTEM_UNMANAGED_REASON_CODE,
            PIP_SYSTEM_UNMANAGED_MESSAGE,
            PIP_SYSTEM_UNMANAGED_SERVICE_ERROR_KEY,
        );
    }

    ManagerEnablementEligibility::eligible()
}

pub fn is_macos_system_rubygems_path_opt(executable_path: Option<&Path>) -> bool {
    executable_path.is_some_and(is_macos_system_rubygems_path)
}

pub fn is_macos_system_rubygems_path(executable_path: &Path) -> bool {
    matches_exact_or_canonical(executable_path, Path::new("/usr/bin/gem"))
}

pub fn is_macos_system_bundler_path_opt(executable_path: Option<&Path>) -> bool {
    executable_path.is_some_and(is_macos_system_bundler_path)
}

pub fn is_macos_system_bundler_path(executable_path: &Path) -> bool {
    matches_exact_or_canonical(executable_path, Path::new("/usr/bin/bundle"))
}

pub fn is_macos_system_pip_path_opt(executable_path: Option<&Path>) -> bool {
    executable_path.is_some_and(is_macos_system_pip_path)
}

pub fn is_macos_system_pip_path(executable_path: &Path) -> bool {
    const MACOS_SYSTEM_PIP_PATHS: &[&str] = &["/usr/bin/python3", "/usr/bin/pip", "/usr/bin/pip3"];
    MACOS_SYSTEM_PIP_PATHS
        .iter()
        .any(|expected| matches_exact_or_canonical(executable_path, Path::new(expected)))
}

fn matches_exact_or_canonical(executable_path: &Path, expected: &Path) -> bool {
    if executable_path == expected {
        return true;
    }

    let normalized = executable_path
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(executable_path));
    normalized == expected
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::models::ManagerId;

    use super::{
        BUNDLER_SYSTEM_UNMANAGED_REASON_CODE, PIP_SYSTEM_UNMANAGED_REASON_CODE,
        RUBYGEMS_SYSTEM_UNMANAGED_REASON_CODE, is_macos_system_bundler_path,
        is_macos_system_pip_path, is_macos_system_rubygems_path, manager_enablement_eligibility,
    };

    #[test]
    fn rubygems_is_ineligible_when_system_executable_is_selected() {
        let eligibility =
            manager_enablement_eligibility(ManagerId::RubyGems, Some(Path::new("/usr/bin/gem")));
        assert!(!eligibility.is_eligible);
        assert_eq!(
            eligibility.reason_code,
            Some(RUBYGEMS_SYSTEM_UNMANAGED_REASON_CODE)
        );
    }

    #[test]
    fn rubygems_is_eligible_with_non_system_executable() {
        let eligibility = manager_enablement_eligibility(
            ManagerId::RubyGems,
            Some(Path::new("/opt/homebrew/bin/gem")),
        );
        assert!(eligibility.is_eligible);
        assert!(eligibility.reason_code.is_none());
    }

    #[test]
    fn non_rubygems_managers_are_unaffected() {
        let eligibility =
            manager_enablement_eligibility(ManagerId::Npm, Some(Path::new("/usr/bin/gem")));
        assert!(eligibility.is_eligible);
    }

    #[test]
    fn bundler_is_ineligible_when_system_executable_is_selected() {
        let eligibility =
            manager_enablement_eligibility(ManagerId::Bundler, Some(Path::new("/usr/bin/bundle")));
        assert!(!eligibility.is_eligible);
        assert_eq!(
            eligibility.reason_code,
            Some(BUNDLER_SYSTEM_UNMANAGED_REASON_CODE)
        );
    }

    #[test]
    fn pip_is_ineligible_when_system_python_or_pip_is_selected() {
        let python_eligibility =
            manager_enablement_eligibility(ManagerId::Pip, Some(Path::new("/usr/bin/python3")));
        assert!(!python_eligibility.is_eligible);
        assert_eq!(
            python_eligibility.reason_code,
            Some(PIP_SYSTEM_UNMANAGED_REASON_CODE)
        );

        let pip_eligibility =
            manager_enablement_eligibility(ManagerId::Pip, Some(Path::new("/usr/bin/pip3")));
        assert!(!pip_eligibility.is_eligible);
        assert_eq!(
            pip_eligibility.reason_code,
            Some(PIP_SYSTEM_UNMANAGED_REASON_CODE)
        );
    }

    #[test]
    fn path_predicate_matches_exact_system_path() {
        assert!(is_macos_system_rubygems_path(Path::new("/usr/bin/gem")));
        assert!(is_macos_system_bundler_path(Path::new("/usr/bin/bundle")));
        assert!(is_macos_system_pip_path(Path::new("/usr/bin/python3")));
    }
}
