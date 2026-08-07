use std::cmp::Ordering;
use std::path::PathBuf;
use std::time::Duration;

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
    OutdatedPackage, PackageRef, TaskId, TaskType,
};

const SPARKLE_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
];

const SPARKLE_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Sparkle,
    display_name: "Sparkle apps",
    category: ManagerCategory::GuiApp,
    authority: ManagerAuthority::Standard,
    capabilities: SPARKLE_CAPABILITIES,
};

const PLUTIL_COMMAND: &str = "/usr/bin/plutil";
const CURL_COMMAND: &str = "/usr/bin/curl";
const SW_VERS_COMMAND: &str = "/usr/bin/sw_vers";
const PLIST_TIMEOUT: Duration = Duration::from_secs(15);
const FEED_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_APPCAST_BYTES: &str = "4194304";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparkleApp {
    pub bundle_path: PathBuf,
    pub bundle_identifier: String,
    pub display_name: String,
    pub short_version: Option<String>,
    pub bundle_version: Option<String>,
    pub feed_url: Option<String>,
}

pub trait SparkleSource: Send + Sync {
    fn apps(&self) -> AdapterResult<Vec<SparkleApp>>;
    fn appcast(&self, feed_url: &str) -> AdapterResult<String>;
    fn system_version(&self) -> AdapterResult<String>;
}

pub struct SparkleAdapter<S: SparkleSource> {
    source: S,
}

impl<S: SparkleSource> SparkleAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    fn snapshots(
        &self,
        apps: &[SparkleApp],
    ) -> AdapterResult<(Vec<InstalledPackage>, Vec<OutdatedPackage>)> {
        let installed = installed_packages(apps);
        let system_version = self.source.system_version()?;
        let mut outdated = Vec::new();

        for app in apps {
            let Some(feed_url) = app.feed_url.as_deref() else {
                continue;
            };
            if !is_secure_feed_url(feed_url) {
                tracing::warn!(
                    bundle_id = app.bundle_identifier,
                    feed_url,
                    "skipping Sparkle appcast because its feed is not a bounded HTTPS URL"
                );
                continue;
            }
            let Some(installed_build) = app.bundle_version.as_deref() else {
                continue;
            };

            let raw = match self.source.appcast(feed_url) {
                Ok(raw) => raw,
                Err(error) => {
                    tracing::warn!(
                        bundle_id = app.bundle_identifier,
                        error = error.message,
                        "Sparkle appcast refresh failed for one application"
                    );
                    continue;
                }
            };
            let candidate = match parse_sparkle_appcast(&raw, installed_build, &system_version) {
                Ok(candidate) => candidate,
                Err(error) => {
                    tracing::warn!(
                        bundle_id = app.bundle_identifier,
                        error = error.message,
                        "Sparkle appcast parsing failed for one application"
                    );
                    continue;
                }
            };
            let Some(candidate) = candidate else {
                continue;
            };

            outdated.push(OutdatedPackage {
                package: package_ref(app),
                package_identifier: Some(app.bundle_path.to_string_lossy().into_owned()),
                installed_version: app
                    .short_version
                    .clone()
                    .or_else(|| app.bundle_version.clone()),
                candidate_version: candidate.display_version,
                pinned: false,
                restart_required: false,
                runtime_state: Default::default(),
            });
        }

        outdated.sort_by(|left, right| left.package.name.cmp(&right.package.name));
        Ok((installed, outdated))
    }
}

impl<S: SparkleSource> ManagerAdapter for SparkleAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &SPARKLE_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let apps = self.source.apps()?;
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed: !apps.is_empty(),
                    // Sparkle is a framework used by apps, not a standalone executable manager.
                    executable_path: None,
                    version: (!apps.is_empty()).then(|| format_app_count(apps.len())),
                }))
            }
            AdapterRequest::Refresh(_) => {
                let apps = self.source.apps()?;
                let (installed, outdated) = self.snapshots(&apps)?;
                Ok(AdapterResponse::SnapshotSync {
                    installed: Some(installed),
                    outdated: Some(outdated),
                })
            }
            AdapterRequest::ListInstalled(_) => {
                let apps = self.source.apps()?;
                Ok(AdapterResponse::InstalledPackages(installed_packages(
                    &apps,
                )))
            }
            AdapterRequest::ListOutdated(_) => {
                let apps = self.source.apps()?;
                let (_, outdated) = self.snapshots(&apps)?;
                Ok(AdapterResponse::OutdatedPackages(outdated))
            }
            _ => unreachable!("unsupported Sparkle request passed capability validation"),
        }
    }
}

fn format_app_count(count: usize) -> String {
    let noun = if count == 1 { "app" } else { "apps" };
    format!("{count} {noun}")
}

fn package_ref(app: &SparkleApp) -> PackageRef {
    PackageRef {
        manager: ManagerId::Sparkle,
        name: app.display_name.clone(),
    }
}

fn installed_packages(apps: &[SparkleApp]) -> Vec<InstalledPackage> {
    let mut packages = apps
        .iter()
        .map(|app| InstalledPackage {
            package: package_ref(app),
            package_identifier: Some(app.bundle_path.to_string_lossy().into_owned()),
            installed_version: app
                .short_version
                .clone()
                .or_else(|| app.bundle_version.clone()),
            pinned: false,
            runtime_state: Default::default(),
        })
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| left.package.name.cmp(&right.package.name));
    packages
}

pub fn sparkle_plist_request(
    task_id: Option<TaskId>,
    info_plist_path: &str,
) -> ProcessSpawnRequest {
    sparkle_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(PLUTIL_COMMAND).args([
            "-convert",
            "json",
            "-o",
            "-",
            "--",
            info_plist_path,
        ]),
        PLIST_TIMEOUT,
    )
}

pub fn sparkle_appcast_request(task_id: Option<TaskId>, feed_url: &str) -> ProcessSpawnRequest {
    sparkle_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(CURL_COMMAND).args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--max-time",
            "40",
            "--max-filesize",
            MAX_APPCAST_BYTES,
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            feed_url,
        ]),
        FEED_TIMEOUT,
    )
}

pub fn sparkle_system_version_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    sparkle_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(SW_VERS_COMMAND).args(["-productVersion"]),
        PLIST_TIMEOUT,
    )
}

fn sparkle_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Sparkle, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn is_secure_feed_url(feed_url: &str) -> bool {
    let trimmed = feed_url.trim();
    trimmed.len() <= 2_048
        && trimmed.starts_with("https://")
        && !trimmed.chars().any(char::is_control)
        && !trimmed.chars().any(char::is_whitespace)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AppcastCandidate {
    build_version: String,
    display_version: String,
}

#[derive(Default)]
struct AppcastItem {
    build_version: Option<String>,
    short_version: Option<String>,
    minimum_system_version: Option<String>,
    maximum_system_version: Option<String>,
    channel: Option<String>,
    os: Option<String>,
}

fn parse_sparkle_appcast(
    xml: &str,
    installed_build: &str,
    system_version: &str,
) -> AdapterResult<Option<AppcastCandidate>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut in_item = false;
    let mut current_field: Option<Vec<u8>> = None;
    let mut item = AppcastItem::default();
    let mut candidates = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let local_name = event.local_name();
                match local_name.as_ref() {
                    b"item" => {
                        in_item = true;
                        item = AppcastItem::default();
                    }
                    b"enclosure" if in_item => {
                        apply_enclosure_attributes(&reader, &event, &mut item)?
                    }
                    name if in_item => current_field = Some(name.to_vec()),
                    _ => {}
                }
            }
            Ok(Event::Empty(event)) if in_item && event.local_name().as_ref() == b"enclosure" => {
                apply_enclosure_attributes(&reader, &event, &mut item)?;
            }
            Ok(Event::Text(text)) if in_item => {
                if let Some(field) = current_field.as_deref() {
                    let decoded = text.decode().map_err(|error| {
                        sparkle_parse_error(format!("invalid appcast text: {error}"))
                    })?;
                    apply_item_text(&mut item, field, decoded.trim());
                }
            }
            Ok(Event::End(event)) if event.local_name().as_ref() == b"item" => {
                if let Some(candidate) = compatible_candidate(item, installed_build, system_version)
                {
                    candidates.push(candidate);
                }
                in_item = false;
                current_field = None;
                item = AppcastItem::default();
            }
            Ok(Event::End(_)) => current_field = None,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(sparkle_parse_error(format!(
                    "invalid Sparkle appcast XML: {error}"
                )));
            }
        }
    }

    candidates.sort_by(|left, right| compare_versions(&left.build_version, &right.build_version));
    Ok(candidates.pop())
}

fn apply_enclosure_attributes(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    item: &mut AppcastItem,
) -> AdapterResult<()> {
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| {
            sparkle_parse_error(format!("invalid Sparkle enclosure attribute: {error}"))
        })?;
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
            .map_err(|error| {
                sparkle_parse_error(format!("invalid Sparkle enclosure value: {error}"))
            })?
            .trim()
            .to_string();
        match attribute.key.local_name().as_ref() {
            b"version" if item.build_version.is_none() => item.build_version = nonempty(value),
            b"shortVersionString" if item.short_version.is_none() => {
                item.short_version = nonempty(value)
            }
            b"os" if item.os.is_none() => item.os = nonempty(value),
            _ => {}
        }
    }
    Ok(())
}

fn apply_item_text(item: &mut AppcastItem, field: &[u8], value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    match field {
        b"version" => item.build_version = Some(value.to_string()),
        b"shortVersionString" => item.short_version = Some(value.to_string()),
        b"minimumSystemVersion" => item.minimum_system_version = Some(value.to_string()),
        b"maximumSystemVersion" => item.maximum_system_version = Some(value.to_string()),
        b"channel" => item.channel = Some(value.to_string()),
        b"os" => item.os = Some(value.to_string()),
        _ => {}
    }
}

fn compatible_candidate(
    item: AppcastItem,
    installed_build: &str,
    system_version: &str,
) -> Option<AppcastCandidate> {
    if item
        .channel
        .as_deref()
        .is_some_and(|channel| !channel.is_empty())
    {
        return None;
    }
    if item
        .os
        .as_deref()
        .is_some_and(|os| !os.eq_ignore_ascii_case("macos"))
    {
        return None;
    }
    if item
        .minimum_system_version
        .as_deref()
        .is_some_and(|minimum| compare_versions(system_version, minimum) == Ordering::Less)
    {
        return None;
    }
    if item
        .maximum_system_version
        .as_deref()
        .is_some_and(|maximum| compare_versions(system_version, maximum) == Ordering::Greater)
    {
        return None;
    }

    let build_version = item.build_version?;
    if compare_versions(&build_version, installed_build) != Ordering::Greater {
        return None;
    }
    let display_version = item.short_version.unwrap_or_else(|| build_version.clone());
    Some(AppcastCandidate {
        build_version,
        display_version,
    })
}

#[derive(Debug, Eq, PartialEq)]
enum VersionPart {
    Number(String),
    Text(String),
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let count = left_parts.len().max(right_parts.len());
    for index in 0..count {
        let ordering = match (left_parts.get(index), right_parts.get(index)) {
            (Some(VersionPart::Number(left)), Some(VersionPart::Number(right))) => {
                compare_numeric_strings(left, right)
            }
            (Some(VersionPart::Text(left)), Some(VersionPart::Text(right))) => left.cmp(right),
            (Some(VersionPart::Number(_)), Some(VersionPart::Text(_))) => Ordering::Greater,
            (Some(VersionPart::Text(_)), Some(VersionPart::Number(_))) => Ordering::Less,
            (Some(VersionPart::Number(value)), None) => compare_numeric_strings(value, "0"),
            (Some(VersionPart::Text(_)), None) => Ordering::Less,
            (None, Some(VersionPart::Number(value))) => compare_numeric_strings("0", value),
            (None, Some(VersionPart::Text(_))) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn version_parts(value: &str) -> Vec<VersionPart> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut current_is_number = None;
    for character in value.trim().chars() {
        let is_number = character.is_ascii_digit();
        if character.is_ascii_alphanumeric() {
            if current_is_number.is_some_and(|kind| kind != is_number) && !current.is_empty() {
                push_version_part(&mut parts, &mut current, current_is_number == Some(true));
            }
            current_is_number = Some(is_number);
            current.push(character.to_ascii_lowercase());
        } else if !current.is_empty() {
            push_version_part(&mut parts, &mut current, current_is_number == Some(true));
            current_is_number = None;
        }
    }
    if !current.is_empty() {
        push_version_part(&mut parts, &mut current, current_is_number == Some(true));
    }
    parts
}

fn push_version_part(parts: &mut Vec<VersionPart>, value: &mut String, is_number: bool) {
    let value = std::mem::take(value);
    if is_number {
        parts.push(VersionPart::Number(value));
    } else {
        parts.push(VersionPart::Text(value));
    }
}

fn compare_numeric_strings(left: &str, right: &str) -> Ordering {
    let left = left.trim_start_matches('0');
    let right = right.trim_start_matches('0');
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn sparkle_parse_error(message: impl Into<String>) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Sparkle),
        task: Some(TaskType::Refresh),
        action: Some(ManagerAction::ListOutdated),
        kind: CoreErrorKind::ParseFailure,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter, RefreshRequest,
    };
    use crate::adapters::sparkle::{
        SparkleAdapter, SparkleApp, SparkleSource, compare_versions, parse_sparkle_appcast,
        sparkle_appcast_request, sparkle_plist_request,
    };
    use crate::models::{ManagerAction, ManagerId, TaskType};

    const APPCAST: &str = r#"<?xml version="1.0"?>
      <rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
        <channel>
          <item>
            <sparkle:version>205</sparkle:version>
            <sparkle:shortVersionString>2.0.5</sparkle:shortVersionString>
            <sparkle:minimumSystemVersion>14.0</sparkle:minimumSystemVersion>
            <enclosure url="https://example.com/app.zip" />
          </item>
          <item>
            <enclosure url="https://example.com/app.zip" sparkle:version="210" sparkle:shortVersionString="2.1.0" />
            <sparkle:minimumSystemVersion>15.0</sparkle:minimumSystemVersion>
          </item>
          <item>
            <sparkle:version>999</sparkle:version>
            <sparkle:shortVersionString>9.9.9 Beta</sparkle:shortVersionString>
            <sparkle:channel>beta</sparkle:channel>
          </item>
        </channel>
      </rss>"#;

    #[test]
    fn appcast_selects_latest_default_channel_compatible_build() {
        let candidate = parse_sparkle_appcast(APPCAST, "200", "14.6")
            .unwrap()
            .expect("candidate");
        assert_eq!(candidate.build_version, "205");
        assert_eq!(candidate.display_version, "2.0.5");
    }

    #[test]
    fn appcast_returns_none_when_installed_build_is_current() {
        assert!(
            parse_sparkle_appcast(APPCAST, "205", "14.6")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn natural_version_comparison_handles_sparkle_build_formats() {
        assert!(compare_versions("14718.1.399", "14718.0.699").is_gt());
        assert!(compare_versions("84367", "83934").is_gt());
        assert!(compare_versions("3.6.11", "3.6.9").is_gt());
        assert!(compare_versions("2.0", "2.0.0").is_eq());
    }

    #[test]
    fn process_requests_are_bounded_and_https_only() {
        let plist = sparkle_plist_request(None, "/Applications/Foo.app/Contents/Info.plist");
        assert_eq!(plist.manager, ManagerId::Sparkle);
        assert_eq!(plist.command.program.to_str(), Some("/usr/bin/plutil"));

        let feed = sparkle_appcast_request(None, "https://example.com/appcast.xml");
        assert_eq!(feed.task_type, TaskType::Refresh);
        assert_eq!(feed.action, ManagerAction::ListOutdated);
        assert!(
            feed.command
                .args
                .windows(2)
                .any(|args| args == ["--proto-redir", "=https"])
        );
        assert!(
            feed.command
                .args
                .windows(2)
                .any(|args| args == ["--max-filesize", "4194304"])
        );
    }

    #[test]
    fn adapter_lists_every_app_and_isolates_feed_failures() {
        let source = FixtureSource {
            apps: vec![
                app(
                    "/Applications/Alpha.app",
                    "com.example.alpha",
                    "Alpha",
                    Some("https://example.com/alpha.xml"),
                ),
                app(
                    "/Applications/Dynamic.app",
                    "com.example.dynamic",
                    "Dynamic",
                    None,
                ),
                app(
                    "/Applications/Offline.app",
                    "com.example.offline",
                    "Offline",
                    Some("https://example.com/offline.xml"),
                ),
            ],
            feeds: HashMap::from([(
                "https://example.com/alpha.xml".to_string(),
                Ok(APPCAST.to_string()),
            )]),
        };
        let adapter = SparkleAdapter::new(source);

        let AdapterResponse::Detection(detection) = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap()
        else {
            panic!("expected detection");
        };
        assert!(detection.installed);
        assert_eq!(detection.version.as_deref(), Some("3 apps"));
        assert_eq!(detection.executable_path, None);

        let AdapterResponse::InstalledPackages(installed) = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap()
        else {
            panic!("expected installed packages");
        };
        assert_eq!(installed.len(), 3);

        let AdapterResponse::OutdatedPackages(outdated) = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap()
        else {
            panic!("expected outdated packages");
        };
        assert_eq!(outdated.len(), 1);
        assert_eq!(outdated[0].package.name, "Alpha");
        assert_eq!(outdated[0].candidate_version, "2.0.5");
        assert_eq!(
            outdated[0].package_identifier.as_deref(),
            Some("/Applications/Alpha.app")
        );
    }

    #[test]
    fn refresh_returns_atomic_installed_and_outdated_snapshots() {
        let source = FixtureSource {
            apps: vec![app(
                "/Applications/Alpha.app",
                "com.example.alpha",
                "Alpha",
                Some("https://example.com/alpha.xml"),
            )],
            feeds: HashMap::from([(
                "https://example.com/alpha.xml".to_string(),
                Ok(APPCAST.to_string()),
            )]),
        };
        let adapter = SparkleAdapter::new(source);
        let AdapterResponse::SnapshotSync {
            installed,
            outdated,
        } = adapter
            .execute(AdapterRequest::Refresh(RefreshRequest))
            .unwrap()
        else {
            panic!("expected snapshot sync");
        };
        assert_eq!(installed.unwrap().len(), 1);
        assert_eq!(outdated.unwrap().len(), 1);
    }

    fn app(
        path: &str,
        bundle_identifier: &str,
        display_name: &str,
        feed_url: Option<&str>,
    ) -> SparkleApp {
        SparkleApp {
            bundle_path: PathBuf::from(path),
            bundle_identifier: bundle_identifier.to_string(),
            display_name: display_name.to_string(),
            short_version: Some("2.0.0".to_string()),
            bundle_version: Some("200".to_string()),
            feed_url: feed_url.map(str::to_string),
        }
    }

    struct FixtureSource {
        apps: Vec<SparkleApp>,
        feeds: HashMap<String, AdapterResult<String>>,
    }

    impl SparkleSource for FixtureSource {
        fn apps(&self) -> AdapterResult<Vec<SparkleApp>> {
            Ok(self.apps.clone())
        }

        fn appcast(&self, feed_url: &str) -> AdapterResult<String> {
            self.feeds
                .get(feed_url)
                .cloned()
                .unwrap_or_else(|| Err(super::sparkle_parse_error("fixture feed unavailable")))
        }

        fn system_version(&self) -> AdapterResult<String> {
            Ok("14.6".to_string())
        }
    }
}
