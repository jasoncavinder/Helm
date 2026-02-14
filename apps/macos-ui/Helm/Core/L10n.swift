import Foundation

struct L10n {
    struct Common {
        static let ok = "common.button.ok"
        static let cancel = "common.button.cancel"
        static let refresh = "common.button.refresh"
        static let install = "common.button.install"
        static let uninstall = "common.button.uninstall"
        static let update = "common.button.update"
        static let settings = "common.button.settings"
        static let quit = "common.button.quit"
        static let done = "common.button.done"
        static let reset = "common.button.reset"
        
        static let version = "common.label.version"
        static let loading = "common.label.loading"
        static let initializing = "common.label.initializing"
        static let error = "common.label.error"
        static let warning = "common.label.warning"
        static let success = "common.label.success"
    }
    
    struct App {
        struct Dashboard {
            static let title = "app.dashboard.title"
            
            struct Section {
                static let recentTasks = "app.dashboard.section.recent_tasks"
                static let managers = "app.dashboard.section.managers"
            }
            
            struct State {
                static let emptyManagers = "app.dashboard.state.empty_managers"
                static let emptyTasks = "app.dashboard.state.empty_tasks"
            }
            
            struct Status {
                static let refreshing = "app.dashboard.status.refreshing"
                static let reconnecting = "app.dashboard.status.reconnecting"
                static let searchRemote = "app.dashboard.status.search_remote"
            }
        }
        
        struct Packages {
            struct Tab {
                static let title = "app.packages.tab.title"
            }
            struct Filter {
                static let installed = "app.packages.filter.installed"
                static let upgradable = "app.packages.filter.upgradable"
                static let available = "app.packages.filter.available"
            }
            struct Detail {
                struct Version {
                    static let current = "app.packages.detail.version.current"
                    static let latest = "app.packages.detail.version.latest"
                    static let pinned = "app.packages.detail.version.pinned"
                }
            }
            struct Action {
                static let install = "app.packages.action.install"
                static let update = "app.packages.action.update"
                static let uninstall = "app.packages.action.uninstall"
            }
        }
        
        struct Managers {
            struct Tab {
                static let title = "app.managers.tab.title"
            }
        }
        
        struct Settings {
            struct Tab {
                static let title = "app.settings.tab.title"
            }
            struct Section {
                static let general = "app.settings.section.general"
                static let managers = "app.settings.section.managers"
                static let advanced = "app.settings.section.advanced"
            }
            struct Label {
                static let language = "app.settings.label.language"
                static let systemDefault = "app.settings.label.language.system_default"
                static let autoCheck = "app.settings.label.auto_check"
                static let checkFrequency = "app.settings.label.check_frequency"
                static let restartRequired = "app.packages.label.restart_required"
            }
            struct Action {
                static let refreshNow = "app.settings.action.refresh_now"
                static let upgradeAll = "app.settings.action.upgrade_all"
                static let reset = "app.settings.action.reset"
                static let quit = "app.settings.action.quit"
            }
            struct Alert {
                struct Reset {
                    static let title = "app.settings.alert.reset.title"
                    static let message = "app.settings.alert.reset.message"
                }
            }
        }
    }
    
    struct Service {
        struct Error {
            static let notInstalled = "service.error.not_installed"
            static let unsupportedCapability = "service.error.unsupported_capability"
            static let invalidInput = "service.error.invalid_input"
            static let parseFailure = "service.error.parse_failure"
            static let timeout = "service.error.timeout"
            static let cancelled = "service.error.cancelled"
            static let processFailure = "service.error.process_failure"
            static let storageFailure = "service.error.storage_failure"
            static let internalError = "service.error.internal"
        }
        struct Task {
            struct Status {
                static let pending = "service.task.status.pending"
                static let running = "service.task.status.running"
                static let completed = "service.task.status.completed"
                static let failed = "service.task.status.failed"
                static let cancelled = "service.task.status.cancelled"
            }
        }
    }
}
