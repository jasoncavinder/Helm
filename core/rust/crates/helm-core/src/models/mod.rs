pub mod error;
pub mod manager;
pub mod package;
pub mod pin;
pub mod search;
pub mod task;

pub use error::{CoreError, CoreErrorKind};
pub use manager::{
    ActionSafety, Capability, DetectionInfo, ManagerAction, ManagerAuthority, ManagerCategory,
    ManagerDescriptor, ManagerId,
};
pub use package::{InstalledPackage, OutdatedPackage, PackageCandidate, PackageRef};
pub use pin::{PinKind, PinRecord};
pub use search::{CachedSearchResult, SearchQuery};
pub use task::{TaskId, TaskRecord, TaskStatus, TaskType};
