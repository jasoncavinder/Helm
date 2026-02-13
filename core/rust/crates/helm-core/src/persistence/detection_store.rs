use crate::models::{DetectionInfo, ManagerId};
use crate::persistence::PersistenceResult;

pub trait DetectionStore: Send + Sync {
    fn upsert_detection(&self, manager: ManagerId, info: &DetectionInfo) -> PersistenceResult<()>;

    fn list_detections(&self) -> PersistenceResult<Vec<(ManagerId, DetectionInfo)>>;

    fn set_manager_enabled(&self, manager: ManagerId, enabled: bool) -> PersistenceResult<()>;

    fn list_manager_preferences(&self) -> PersistenceResult<Vec<(ManagerId, bool)>>;
}
