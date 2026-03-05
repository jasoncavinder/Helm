use crate::models::{
    DetectionInfo, HomebrewKegPolicy, ManagerId, ManagerInstallInstance, PackageKegPolicy,
    PackageRef,
};
use crate::persistence::PersistenceResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerPreference {
    pub manager: ManagerId,
    pub enabled: bool,
    pub selected_executable_path: Option<String>,
    pub selected_install_method: Option<String>,
    pub timeout_hard_seconds: Option<u64>,
    pub timeout_idle_seconds: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageManagerPreference {
    pub package_name: String,
    pub manager: ManagerId,
}

pub trait DetectionStore: Send + Sync {
    fn upsert_detection(&self, manager: ManagerId, info: &DetectionInfo) -> PersistenceResult<()>;

    fn list_detections(&self) -> PersistenceResult<Vec<(ManagerId, DetectionInfo)>>;

    fn replace_install_instances(
        &self,
        manager: ManagerId,
        instances: &[ManagerInstallInstance],
    ) -> PersistenceResult<()>;

    fn list_install_instances(
        &self,
        manager: Option<ManagerId>,
    ) -> PersistenceResult<Vec<ManagerInstallInstance>>;

    fn set_manager_multi_instance_ack_fingerprint(
        &self,
        manager: ManagerId,
        fingerprint: Option<&str>,
    ) -> PersistenceResult<()>;

    fn manager_multi_instance_ack_fingerprint(
        &self,
        manager: ManagerId,
    ) -> PersistenceResult<Option<String>>;

    fn set_manager_enabled(&self, manager: ManagerId, enabled: bool) -> PersistenceResult<()>;

    fn set_manager_selected_executable_path(
        &self,
        manager: ManagerId,
        path: Option<&str>,
    ) -> PersistenceResult<()>;

    fn set_manager_selected_install_method(
        &self,
        manager: ManagerId,
        method: Option<&str>,
    ) -> PersistenceResult<()>;

    fn set_manager_timeout_hard_seconds(
        &self,
        manager: ManagerId,
        seconds: Option<u64>,
    ) -> PersistenceResult<()>;

    fn set_manager_timeout_idle_seconds(
        &self,
        manager: ManagerId,
        seconds: Option<u64>,
    ) -> PersistenceResult<()>;

    fn list_manager_preferences(&self) -> PersistenceResult<Vec<ManagerPreference>>;

    fn set_safe_mode(&self, enabled: bool) -> PersistenceResult<()>;

    fn safe_mode(&self) -> PersistenceResult<bool>;

    fn set_homebrew_keg_policy(&self, policy: HomebrewKegPolicy) -> PersistenceResult<()>;

    fn homebrew_keg_policy(&self) -> PersistenceResult<HomebrewKegPolicy>;

    fn set_auto_check_for_updates(&self, enabled: bool) -> PersistenceResult<()>;

    fn auto_check_for_updates(&self) -> PersistenceResult<bool>;

    fn set_auto_check_frequency_minutes(&self, minutes: u32) -> PersistenceResult<()>;

    fn auto_check_frequency_minutes(&self) -> PersistenceResult<u32>;

    fn set_auto_check_last_checked_unix(&self, value: i64) -> PersistenceResult<()>;

    fn auto_check_last_checked_unix(&self) -> PersistenceResult<Option<i64>>;

    fn set_cli_onboarding_completed(&self, completed: bool) -> PersistenceResult<()>;

    fn cli_onboarding_completed(&self) -> PersistenceResult<bool>;

    fn set_cli_accepted_license_terms_version(
        &self,
        version: Option<&str>,
    ) -> PersistenceResult<()>;

    fn cli_accepted_license_terms_version(&self) -> PersistenceResult<Option<String>>;

    fn set_manager_priority_overrides_json(
        &self,
        overrides_json: Option<&str>,
    ) -> PersistenceResult<()>;

    fn manager_priority_overrides_json(&self) -> PersistenceResult<Option<String>>;

    fn set_package_keg_policy(
        &self,
        package: &PackageRef,
        policy: Option<HomebrewKegPolicy>,
    ) -> PersistenceResult<()>;

    fn package_keg_policy(
        &self,
        package: &PackageRef,
    ) -> PersistenceResult<Option<HomebrewKegPolicy>>;

    fn list_package_keg_policies(&self) -> PersistenceResult<Vec<PackageKegPolicy>>;

    fn set_package_manager_preference(
        &self,
        package_name: &str,
        manager: Option<ManagerId>,
    ) -> PersistenceResult<()>;

    fn package_manager_preference(
        &self,
        package_name: &str,
    ) -> PersistenceResult<Option<ManagerId>>;

    fn list_package_manager_preferences(&self) -> PersistenceResult<Vec<PackageManagerPreference>>;
}
