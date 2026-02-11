use std::collections::HashSet;
use std::path::PathBuf;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery,
};

const HOMEBREW_READ_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
];

const HOMEBREW_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::HomebrewFormula,
    display_name: "Homebrew (formulae)",
    category: ManagerCategory::SystemOs,
    authority: ManagerAuthority::Guarded,
    capabilities: HOMEBREW_READ_CAPABILITIES,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomebrewDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait HomebrewSource: Send + Sync {
    fn detect(&self) -> AdapterResult<HomebrewDetectOutput>;

    fn list_installed_formulae(&self) -> AdapterResult<String>;

    fn list_outdated_formulae(&self) -> AdapterResult<String>;

    fn search_local_formulae(&self, query: &str) -> AdapterResult<String>;
}

pub struct HomebrewAdapter<S: HomebrewSource> {
    source: S,
}

impl<S: HomebrewSource> HomebrewAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: HomebrewSource> ManagerAdapter for HomebrewAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &HOMEBREW_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let detect_output = self.source.detect()?;
                Ok(AdapterResponse::Detection(parse_detection_output(
                    detect_output,
                )))
            }
            AdapterRequest::Refresh(_) => {
                let _ = self.source.detect()?;
                Ok(AdapterResponse::Refreshed)
            }
            AdapterRequest::ListInstalled(_) => {
                let raw = self.source.list_installed_formulae()?;
                let packages = parse_installed_formulae(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated_formulae()?;
                let packages = parse_outdated_formulae(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self
                    .source
                    .search_local_formulae(search_request.query.text.as_str())?;
                let results = parse_search_formulae(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::HomebrewFormula),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "homebrew adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

fn parse_detection_output(output: HomebrewDetectOutput) -> DetectionInfo {
    let parsed_version = parse_homebrew_version(&output.version_output);
    let has_executable = output.executable_path.is_some();
    let installed = has_executable || parsed_version.is_some();

    DetectionInfo {
        installed,
        executable_path: output.executable_path,
        version: parsed_version,
    }
}

fn parse_homebrew_version(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(|line| line.strip_prefix("Homebrew ").map(str::to_owned))
}

fn parse_installed_formulae(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut parsed = Vec::new();
    let mut malformed_lines = 0usize;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        match parse_installed_line(line) {
            Some((name, version)) => parsed.push(InstalledPackage {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name,
                },
                installed_version: version,
                pinned: false,
            }),
            None => malformed_lines += 1,
        }
    }

    if parsed.is_empty() && malformed_lines > 0 {
        return Err(parse_error(
            "unable to parse any installed Homebrew formulae lines",
        ));
    }

    Ok(parsed)
}

fn parse_installed_line(line: &str) -> Option<(String, Option<String>)> {
    let mut segments = line.split_whitespace();
    let name = segments.next()?.trim();
    if name.is_empty() {
        return None;
    }

    // `brew list --formula --versions` may print multiple installed versions;
    // normalize to the latest token as the active version.
    let version = segments.last().map(str::to_owned);
    Some((name.to_owned(), version))
}

fn parse_outdated_formulae(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut parsed = Vec::new();
    let mut malformed_lines = 0usize;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        match parse_outdated_line(line) {
            Some((name, installed_version, candidate_version)) => parsed.push(OutdatedPackage {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name,
                },
                installed_version,
                candidate_version,
                pinned: false,
            }),
            None => malformed_lines += 1,
        }
    }

    if parsed.is_empty() && malformed_lines > 0 {
        return Err(parse_error(
            "unable to parse any outdated Homebrew formulae lines",
        ));
    }

    Ok(parsed)
}

fn parse_outdated_line(line: &str) -> Option<(String, Option<String>, String)> {
    let (left, right) = line.split_once(" -> ")?;
    let mut left_segments = left.split_whitespace();
    let name = left_segments.next()?.trim();
    if name.is_empty() {
        return None;
    }

    let installed_version = left_segments.last().map(str::to_owned);
    let candidate_version = right.trim();
    if candidate_version.is_empty() {
        return None;
    }

    Some((
        name.to_owned(),
        installed_version,
        candidate_version.to_owned(),
    ))
}

fn parse_search_formulae(
    output: &str,
    query: &SearchQuery,
) -> AdapterResult<Vec<CachedSearchResult>> {
    let mut parsed = Vec::new();
    let mut seen = HashSet::new();
    let mut section = SearchSection::Unspecified;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(next) = parse_search_section_header(line) {
            section = next;
            continue;
        }

        if section == SearchSection::Casks {
            continue;
        }

        if is_no_results_diagnostic(line) {
            continue;
        }

        for token in line.split_whitespace() {
            if !is_formula_name_token(token) {
                continue;
            }

            if seen.insert(token.to_string()) {
                parsed.push(CachedSearchResult {
                    result: PackageCandidate {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: token.to_string(),
                        },
                        version: None,
                        summary: None,
                    },
                    source_manager: ManagerId::HomebrewFormula,
                    originating_query: query.text.clone(),
                    cached_at: query.issued_at,
                });
            }
        }
    }

    Ok(parsed)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SearchSection {
    Unspecified,
    Formulae,
    Casks,
}

fn parse_search_section_header(line: &str) -> Option<SearchSection> {
    if !line.starts_with("==>") {
        return None;
    }
    let lowered = line.to_ascii_lowercase();
    if lowered.contains("formula") {
        return Some(SearchSection::Formulae);
    }
    if lowered.contains("cask") {
        return Some(SearchSection::Casks);
    }
    Some(SearchSection::Unspecified)
}

fn is_formula_name_token(token: &str) -> bool {
    if token.is_empty() || token.starts_with("==>") {
        return false;
    }
    token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '+' | '-' | '_' | '.' | '/'))
}

fn is_no_results_diagnostic(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    lowered.starts_with("no formulae or casks found for")
        || lowered.starts_with("no formula or cask found for")
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::HomebrewFormula),
        task: None,
        action: None,
        kind: CoreErrorKind::ParseFailure,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::UNIX_EPOCH;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter, SearchRequest,
    };
    use crate::models::{CoreErrorKind, SearchQuery};

    use super::{
        HomebrewAdapter, HomebrewDetectOutput, HomebrewSource, parse_homebrew_version,
        parse_installed_formulae, parse_outdated_formulae, parse_search_formulae,
    };

    const INSTALLED_FIXTURE: &str =
        include_str!("../../tests/fixtures/homebrew/list_installed_versions.txt");
    const OUTDATED_FIXTURE: &str =
        include_str!("../../tests/fixtures/homebrew/list_outdated_verbose.txt");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/homebrew/search_local.txt");

    #[test]
    fn parses_homebrew_version_from_standard_banner() {
        let version = parse_homebrew_version("Homebrew 4.2.21\n");
        assert_eq!(version.as_deref(), Some("4.2.21"));
    }

    #[test]
    fn detection_marks_not_installed_when_probe_has_no_signals() {
        let detection = super::parse_detection_output(HomebrewDetectOutput {
            executable_path: None,
            version_output: String::new(),
        });
        assert!(!detection.installed);
    }

    #[test]
    fn parses_installed_formulae_fixture() {
        let parsed = parse_installed_formulae(INSTALLED_FIXTURE).unwrap();
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].package.name, "python@3.12");
        assert_eq!(parsed[0].installed_version.as_deref(), Some("3.12.3"));
        assert_eq!(parsed[2].package.name, "node");
        assert_eq!(parsed[2].installed_version.as_deref(), Some("22.5.1"));
    }

    #[test]
    fn parses_outdated_formulae_fixture() {
        let parsed = parse_outdated_formulae(OUTDATED_FIXTURE).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].package.name, "git");
        assert_eq!(parsed[0].installed_version.as_deref(), Some("2.44.0"));
        assert_eq!(parsed[0].candidate_version, "2.45.1");
    }

    #[test]
    fn returns_parse_error_for_fully_malformed_outdated_output() {
        let error = parse_outdated_formulae("this-is-not-parseable").unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
    }

    #[test]
    fn search_ignores_no_result_diagnostic_line() {
        let query = SearchQuery {
            text: "foo".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let parsed =
            parse_search_formulae("No formulae or casks found for \"foo\".", &query).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn parses_search_formulae_fixture() {
        let query = SearchQuery {
            text: "rip".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let parsed = parse_search_formulae(SEARCH_FIXTURE, &query).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].result.package.name, "ripgrep");
        assert_eq!(parsed[1].result.package.name, "ripgrep-all");
        assert_eq!(parsed[2].result.package.name, "ripsecret");
    }

    #[test]
    fn adapter_executes_supported_read_only_requests() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let installed = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();
        let outdated = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();
        let search = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "rip".to_string(),
                    issued_at: UNIX_EPOCH,
                },
            }))
            .unwrap();

        assert!(matches!(detect, AdapterResponse::Detection(_)));
        assert!(matches!(installed, AdapterResponse::InstalledPackages(_)));
        assert!(matches!(outdated, AdapterResponse::OutdatedPackages(_)));
        assert!(matches!(search, AdapterResponse::SearchResults(_)));
    }

    #[test]
    fn adapter_rejects_mutating_request_with_unsupported_capability() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: crate::models::ManagerId::HomebrewFormula,
                    name: "ripgrep".to_string(),
                },
                version: None,
            }))
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    }

    #[derive(Default, Clone)]
    struct FixtureSource {
        detect_calls: Arc<AtomicUsize>,
    }

    impl HomebrewSource for FixtureSource {
        fn detect(&self) -> AdapterResult<HomebrewDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            Ok(HomebrewDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                version_output: "Homebrew 4.2.21".to_string(),
            })
        }

        fn list_installed_formulae(&self) -> AdapterResult<String> {
            Ok(INSTALLED_FIXTURE.to_string())
        }

        fn list_outdated_formulae(&self) -> AdapterResult<String> {
            Ok(OUTDATED_FIXTURE.to_string())
        }

        fn search_local_formulae(&self, _query: &str) -> AdapterResult<String> {
            Ok(SEARCH_FIXTURE.to_string())
        }
    }
}
