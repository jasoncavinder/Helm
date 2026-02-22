use crate::models::{DetectionInfo, HomebrewKegPolicy, ManagerId, PackageKegPolicy, PackageRef};
use crate::persistence::PersistenceResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerPreference {
    pub manager: ManagerId,
    pub enabled: bool,
    pub selected_executable_path: Option<String>,
    pub selected_install_method: Option<String>,
}

pub trait DetectionStore: Send + Sync {
    fn upsert_detection(&self, manager: ManagerId, info: &DetectionInfo) -> PersistenceResult<()>;

    fn list_detections(&self) -> PersistenceResult<Vec<(ManagerId, DetectionInfo)>>;

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

    fn list_manager_preferences(&self) -> PersistenceResult<Vec<ManagerPreference>>;

    fn set_safe_mode(&self, enabled: bool) -> PersistenceResult<()>;

    fn safe_mode(&self) -> PersistenceResult<bool>;

    fn set_homebrew_keg_policy(&self, policy: HomebrewKegPolicy) -> PersistenceResult<()>;

    fn homebrew_keg_policy(&self) -> PersistenceResult<HomebrewKegPolicy>;

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
}
