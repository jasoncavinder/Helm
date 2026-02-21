pub mod error;
pub mod keg_policy;
pub mod manager;
pub mod package;
pub mod pin;
pub mod search;
pub mod task;
pub mod task_log;

pub use error::{CoreError, CoreErrorKind};
pub use keg_policy::{HomebrewKegPolicy, PackageKegPolicy};
pub use manager::{
    ActionSafety, Capability, DetectionInfo, ManagerAction, ManagerAuthority, ManagerCategory,
    ManagerDescriptor, ManagerId,
};
pub use package::{InstalledPackage, OutdatedPackage, PackageCandidate, PackageRef};
pub use pin::{PinKind, PinRecord};
pub use search::{CachedSearchResult, SearchQuery};
pub use task::{TaskId, TaskRecord, TaskStatus, TaskType};
pub use task_log::{NewTaskLogRecord, TaskLogLevel, TaskLogRecord};
