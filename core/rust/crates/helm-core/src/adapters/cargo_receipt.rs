use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::adapters::manager::AdapterResult;
use crate::execution::CommandSpec;
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId};

const MAX_RECEIPT_BYTES: u64 = 4 * 1024 * 1024;
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CargoInstallReceipt {
    // Native Cargo writes null when the original install omitted --version.
    // Keep the field required while accepting that complete native shape.
    #[serde(deserialize_with = "required_version_req")]
    pub version_req: Option<String>,
    pub bins: BTreeSet<String>,
    pub features: BTreeSet<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub profile: String,
    pub target: String,
    pub rustc: String,
}

fn required_version_req<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoReceipts {
    #[serde(deserialize_with = "unique_receipts")]
    installs: BTreeMap<String, CargoInstallReceipt>,
}

fn unique_receipts<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, CargoInstallReceipt>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, CargoInstallReceipt>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("unique Cargo installation receipts")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut receipts = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, CargoInstallReceipt>()? {
                if receipts.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate Cargo receipt"));
                }
            }
            Ok(receipts)
        }
    }
    deserializer.deserialize_map(Visitor)
}

#[derive(Debug)]
pub(crate) struct CargoUpgradeReceipt {
    root: PathBuf,
    name: String,
    installed_key: String,
    receipt: CargoInstallReceipt,
    all: BTreeMap<String, CargoInstallReceipt>,
}

pub(crate) fn receipt_error(detail: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Cargo),
        task: None,
        action: Some(ManagerAction::Upgrade),
        kind: CoreErrorKind::UnsupportedCapability,
        message: format!("[cargo_receipt_unsupported] {detail}"),
    }
}

fn read_bounded(path: &Path) -> AdapterResult<Vec<u8>> {
    let metadata = std::fs::metadata(path)
        .map_err(|_| receipt_error("Cargo installation metadata is unavailable"))?;
    if !metadata.is_file() || metadata.len() > MAX_RECEIPT_BYTES {
        return Err(receipt_error(
            "Cargo installation metadata is not a bounded regular file",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_RECEIPT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| receipt_error("Cargo installation metadata is unavailable"))?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(receipt_error(
            "Cargo installation metadata exceeds the supported size",
        ));
    }
    Ok(bytes)
}

fn read_receipts(root: &Path) -> AdapterResult<BTreeMap<String, CargoInstallReceipt>> {
    let bytes = read_bounded(&root.join(".crates2.json"))?;
    let receipts: CargoReceipts = serde_json::from_slice(&bytes)
        .map_err(|_| receipt_error("Cargo installation metadata is incomplete or unsupported"))?;
    Ok(receipts.installs)
}

fn simple_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.+-".contains(&byte))
}

fn key_parts(key: &str) -> Option<(&str, &str, &str)> {
    let (package, source) = key.strip_suffix(')')?.split_once(" (")?;
    let (name, version) = package.split_once(' ')?;
    simple_name(name).then_some(())?;
    semver::Version::parse(version).ok()?;
    Some((name, version, source))
}

impl CargoUpgradeReceipt {
    pub(crate) fn load(root: PathBuf, name: &str) -> AdapterResult<Self> {
        if !root.is_absolute() || root.to_str().is_none() {
            return Err(receipt_error("Cargo install root must be absolute UTF-8"));
        }
        let all = read_receipts(&root)?;
        if all.keys().any(|key| key_parts(key).is_none()) {
            return Err(receipt_error(
                "Cargo receipt identities cannot be validated",
            ));
        }
        let matches: Vec<_> = all
            .iter()
            .filter(|(key, _)| {
                key_parts(key).is_some_and(|(installed_name, _, _)| installed_name == name)
            })
            .collect();
        if matches.len() != 1 {
            return Err(receipt_error(
                "Cargo package receipt is missing or ambiguous",
            ));
        }
        let (key, receipt) = matches[0];
        if key_parts(key).map(|(_, _, source)| source) != Some(CRATES_IO) {
            return Err(receipt_error(
                "Cargo upgrades of Git, path or private-registry installs require manual review",
            ));
        }
        if receipt.bins.is_empty()
            || !receipt.bins.iter().all(|bin| simple_name(bin))
            || !receipt
                .features
                .iter()
                .all(|feature| !feature.is_empty() && feature.split('/').all(simple_name))
            || !simple_name(&receipt.profile)
            || !simple_name(&receipt.target)
            || receipt
                .version_req
                .as_deref()
                .is_some_and(|requirement| semver::VersionReq::parse(requirement).is_err())
        {
            return Err(receipt_error(
                "Cargo build options cannot be preserved safely",
            ));
        }
        for bin in &receipt.bins {
            let path = root.join("bin").join(bin);
            if !std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()) {
                return Err(receipt_error(
                    "Cargo receipt does not match its installed binaries",
                ));
            }
            if all
                .iter()
                .any(|(other_key, other)| other_key != key && other.bins.contains(bin))
            {
                return Err(receipt_error("Cargo binary ownership is ambiguous"));
            }
        }
        Ok(Self {
            root,
            name: name.to_string(),
            installed_key: key.clone(),
            receipt: receipt.clone(),
            all,
        })
    }

    pub(crate) fn apply(&self, mut command: CommandSpec) -> CommandSpec {
        command = command.args([
            "--root",
            &self.root.to_string_lossy(),
            "--registry",
            "crates-io",
        ]);
        for bin in &self.receipt.bins {
            command = command.args(["--bin", bin]);
        }
        for feature in &self.receipt.features {
            command = command.args(["--features", feature]);
        }
        if self.receipt.all_features {
            command = command.arg("--all-features");
        }
        if self.receipt.no_default_features {
            command = command.arg("--no-default-features");
        }
        command.args([
            "--profile",
            &self.receipt.profile,
            "--target",
            &self.receipt.target,
        ])
    }

    pub(crate) fn revalidate(&self) -> AdapterResult<()> {
        if read_receipts(&self.root)? != self.all {
            return Err(receipt_error(
                "Cargo installation changed before execution; review again",
            ));
        }
        Ok(())
    }

    pub(crate) fn verify(&self, version: &str) -> AdapterResult<()> {
        let after = Self::load(self.root.clone(), &self.name)?;
        let Some((_, actual_version, _)) = key_parts(&after.installed_key) else {
            return Err(receipt_error("Cargo post-install receipt is invalid"));
        };
        let expected = &self.receipt;
        let actual = &after.receipt;
        let mut before_others = self.all.clone();
        before_others.remove(&self.installed_key);
        let mut after_others = after.all;
        after_others.remove(&after.installed_key);
        if actual_version != version
            || actual.bins != expected.bins
            || actual.features != expected.features
            || actual.all_features != expected.all_features
            || actual.no_default_features != expected.no_default_features
            || actual.profile != expected.profile
            || actual.target != expected.target
            || before_others != after_others
        {
            return Err(CoreError {
                kind: CoreErrorKind::ProcessFailure,
                message: "[cargo_receipt_unsupported] Cargo completed but the expected source, build options or unrelated receipts could not be verified".into(),
                ..receipt_error("")
            });
        }
        Ok(())
    }
}

pub(crate) fn install_root(cargo_home: &Path, explicit: Option<PathBuf>) -> AdapterResult<PathBuf> {
    let mut configured = None;
    // Cargo gives the extensionless legacy file precedence when both exist.
    let legacy = cargo_home.join("config");
    let config = if legacy
        .try_exists()
        .map_err(|_| receipt_error("Cargo configuration is inaccessible"))?
    {
        legacy
    } else {
        cargo_home.join("config.toml")
    };
    if config
        .try_exists()
        .map_err(|_| receipt_error("Cargo configuration is inaccessible"))?
    {
        let bytes = read_bounded(&config)?;
        let config: toml::Value = toml::from_str(
            std::str::from_utf8(&bytes)
                .map_err(|_| receipt_error("Cargo configuration is not UTF-8"))?,
        )
        .map_err(|_| receipt_error("Cargo configuration cannot be validated"))?;
        if config.get("source").is_some()
            || config.get("include").is_some()
            || config
                .get("registry")
                .and_then(|v| v.get("default"))
                .is_some_and(|value| value.as_str() != Some("crates-io"))
            || config
                .get("registries")
                .and_then(|v| v.get("crates-io"))
                .is_some()
        {
            return Err(receipt_error(
                "Cargo source overrides require manual review",
            ));
        }
        if let Some(root) = config.get("install").and_then(|value| value.get("root")) {
            configured =
                Some(PathBuf::from(root.as_str().ok_or_else(|| {
                    receipt_error("Cargo install root is not a path")
                })?));
        }
    }
    let root = explicit
        .or(configured)
        .unwrap_or_else(|| cargo_home.to_path_buf());
    if !root.is_absolute()
        || root.to_str().is_none()
        || root
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(receipt_error(
            "Cargo install root must be an absolute unambiguous path",
        ));
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn entry() -> Value {
        json!({"version_req":"=1.0.0", "bins":["tool"], "features":["color"],
            "all_features":false, "no_default_features":true, "profile":"release",
            "target":"aarch64-apple-darwin", "rustc":"rustc 1.98.1"})
    }

    #[test]
    fn non_file_metadata_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".crates2.json")).unwrap();
        assert!(CargoUpgradeReceipt::load(root.path().into(), "tool").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_install_roots_cannot_be_lossily_retargeted() {
        use std::os::unix::ffi::OsStringExt;
        let root = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/root-\xff".to_vec()));
        assert!(CargoUpgradeReceipt::load(root.clone(), "tool").is_err());
        assert!(install_root(Path::new("/tmp/cargo"), Some(root)).is_err());
    }

    fn save(root: &Path, version: &str, source: &str, entry: Value) {
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/tool"), "binary").unwrap();
        std::fs::write(
            root.join(".crates2.json"),
            serde_json::to_vec(&json!({
                "installs": {format!("tool {version} ({source})"): entry}
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn upgrade_preserves_explicit_receipt_options_and_verifies_new_version() {
        let root = tempfile::tempdir().unwrap();
        let mut original = entry();
        original["all_features"] = json!(true);
        save(root.path(), "1.0.0", CRATES_IO, original.clone());
        let plan = CargoUpgradeReceipt::load(root.path().into(), "tool").unwrap();
        let command = plan.apply(CommandSpec::new("cargo"));
        assert_eq!(
            command.args,
            [
                "--root",
                root.path().to_str().unwrap(),
                "--registry",
                "crates-io",
                "--bin",
                "tool",
                "--features",
                "color",
                "--all-features",
                "--no-default-features",
                "--profile",
                "release",
                "--target",
                "aarch64-apple-darwin"
            ]
        );
        plan.revalidate().unwrap();
        original["version_req"] = json!("=2.0.0");
        original["rustc"] = json!("rustc 1.99.0");
        save(root.path(), "2.0.0", CRATES_IO, original);
        plan.verify("2.0.0").unwrap();
        assert!(plan.revalidate().is_err());
        assert!(plan.verify("3.0.0").is_err());
    }

    #[test]
    fn native_unversioned_install_receipt_can_be_upgraded() {
        let root = tempfile::tempdir().unwrap();
        let mut original = entry();
        original["version_req"] = Value::Null;
        save(root.path(), "1.0.0", CRATES_IO, original.clone());
        let plan = CargoUpgradeReceipt::load(root.path().into(), "tool").unwrap();
        plan.revalidate().unwrap();
        let command =
            plan.apply(CommandSpec::new("cargo").args(["install", "tool", "--version", "=2.0.0"]));
        assert!(
            command
                .args
                .windows(2)
                .any(|args| args == ["--version", "=2.0.0"])
        );
        original["version_req"] = json!("=2.0.0");
        save(root.path(), "2.0.0", CRATES_IO, original);
        plan.verify("2.0.0").unwrap();
    }

    #[test]
    fn unsupported_sources_and_incomplete_receipts_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        for source in [
            "git+https://example.test/repo#abc",
            "path+file:///tmp/tool",
            "registry+https://example.test/index",
        ] {
            save(root.path(), "1.0.0", source, entry());
            assert!(CargoUpgradeReceipt::load(root.path().into(), "tool").is_err());
        }
        for field in [
            "version_req",
            "features",
            "bins",
            "profile",
            "target",
            "all_features",
            "no_default_features",
        ] {
            let mut incomplete = entry();
            incomplete.as_object_mut().unwrap().remove(field);
            save(root.path(), "1.0.0", CRATES_IO, incomplete);
            assert!(
                CargoUpgradeReceipt::load(root.path().into(), "tool").is_err(),
                "{field}"
            );
        }
        save(root.path(), "1.0.0", CRATES_IO, entry());
        assert!(CargoUpgradeReceipt::load(root.path().into(), "missing").is_err());
        for requirement in [json!("not a requirement"), json!(42), json!([])] {
            let mut invalid = entry();
            invalid["version_req"] = requirement;
            save(root.path(), "1.0.0", CRATES_IO, invalid);
            assert!(CargoUpgradeReceipt::load(root.path().into(), "tool").is_err());
        }
    }

    #[test]
    fn option_and_source_drift_never_report_verified_success() {
        let root = tempfile::tempdir().unwrap();
        save(root.path(), "1.0.0", CRATES_IO, entry());
        let plan = CargoUpgradeReceipt::load(root.path().into(), "tool").unwrap();
        for (field, value) in [
            ("features", json!([])),
            ("profile", json!("dev")),
            ("target", json!("x86_64-apple-darwin")),
            ("no_default_features", json!(false)),
            ("all_features", json!(true)),
            ("bins", json!([])),
        ] {
            let mut changed = entry();
            changed[field] = value;
            save(root.path(), "2.0.0", CRATES_IO, changed);
            assert!(plan.verify("2.0.0").is_err(), "{field}");
        }
        save(root.path(), "2.0.0", "path+file:///tmp/tool", entry());
        assert!(plan.verify("2.0.0").is_err());
    }

    #[test]
    fn ambiguous_ownership_and_duplicate_receipts_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        save(root.path(), "1.0.0", CRATES_IO, entry());
        let key = format!("tool 1.0.0 ({CRATES_IO})");
        let bytes = format!(
            "{{\"installs\":{{{}:{},{}:{}}}}}",
            json!(key),
            entry(),
            json!(key),
            entry()
        );
        std::fs::write(root.path().join(".crates2.json"), bytes).unwrap();
        assert!(CargoUpgradeReceipt::load(root.path().into(), "tool").is_err());
        let records =
            json!({"installs": {key: entry(), format!("other 1.0.0 ({CRATES_IO})"):entry()}});
        std::fs::write(root.path().join(".crates2.json"), records.to_string()).unwrap();
        assert!(CargoUpgradeReceipt::load(root.path().into(), "tool").is_err());
    }

    #[test]
    fn unrelated_receipt_changes_fail_postcondition() {
        let root = tempfile::tempdir().unwrap();
        save(root.path(), "1.0.0", CRATES_IO, entry());
        let plan = CargoUpgradeReceipt::load(root.path().into(), "tool").unwrap();
        let mut other = entry();
        other["bins"] = json!(["other"]);
        std::fs::write(root.path().join(".crates2.json"), json!({"installs": {
            format!("tool 2.0.0 ({CRATES_IO})"):entry(), format!("other 1.0.0 ({CRATES_IO})"):other
        }}).to_string()).unwrap();
        assert!(plan.verify("2.0.0").is_err());
    }

    #[test]
    fn bin_paths_unknown_fields_and_missing_executables_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        for bins in [
            json!(["../tool"]),
            json!(["--help"]),
            json!(["absent"]),
            json!([]),
        ] {
            let mut value = entry();
            value["bins"] = bins;
            save(root.path(), "1.0.0", CRATES_IO, value);
            assert!(CargoUpgradeReceipt::load(root.path().into(), "tool").is_err());
        }
        let mut value = entry();
        value["future_source_option"] = json!(true);
        save(root.path(), "1.0.0", CRATES_IO, value);
        assert!(CargoUpgradeReceipt::load(root.path().into(), "tool").is_err());
    }

    #[test]
    fn root_precedence_and_source_overrides_are_explicit() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(install_root(home.path(), None).unwrap(), home.path());
        std::fs::write(
            home.path().join("config.toml"),
            "[install]\nroot = '/tmp/install-root'\n",
        )
        .unwrap();
        assert_eq!(
            install_root(home.path(), None).unwrap(),
            PathBuf::from("/tmp/install-root")
        );
        assert_eq!(
            install_root(home.path(), Some("/tmp/explicit".into())).unwrap(),
            PathBuf::from("/tmp/explicit")
        );
        for configuration in [
            "[source.crates-io]\nreplace-with = 'mirror'\n",
            "[registry]\ndefault = 'private'\n",
            "[install]\nroot = 'relative'\n",
            "include = 'other.toml'\n",
            "[registries.crates-io]\nindex = 'https://example.test'\n",
            "[broken",
        ] {
            std::fs::write(home.path().join("config.toml"), configuration).unwrap();
            assert!(install_root(home.path(), None).is_err(), "{configuration}");
        }
        std::fs::write(
            home.path().join("config"),
            "[install]\nroot = '/tmp/legacy'\n",
        )
        .unwrap();
        assert_eq!(
            install_root(home.path(), None).unwrap(),
            PathBuf::from("/tmp/legacy")
        );
    }
}
