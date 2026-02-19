import Foundation

extension L10n {
    struct App {
        struct Navigation {
            static let dashboard = "app.navigation.tab.dashboard"
            static let packages = "app.navigation.tab.packages"
            static let managers = "app.navigation.tab.managers"
            static let searchPlaceholder = "app.navigation.search.placeholder"
        }

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
                static let allManagers = "app.packages.filter.all_managers"
                static let installed = "app.packages.filter.installed"
                static let upgradable = "app.packages.filter.upgradable"
                static let available = "app.packages.filter.available"
            }
            struct State {
                static let noPackagesFound = "app.packages.state.no_packages_found"
            }
            struct Detail {
                struct Version {
                    static let current = "app.packages.detail.version.current"
                    static let latest = "app.packages.detail.version.latest"
                    static let pinned = "app.packages.detail.version.pinned"
                }
            }
            struct Label {
                static let restartRequired = "app.packages.label.restart_required"
                static let pinned = "app.packages.label.pinned"
                static let homebrewKegPolicy = "app.packages.label.homebrew_keg_policy"
            }
            struct Action {
                static let install = "app.packages.action.install"
                static let update = "app.packages.action.update"
                static let uninstall = "app.packages.action.uninstall"
                static let pin = "app.packages.action.pin"
                static let unpin = "app.packages.action.unpin"
                static let upgradePackage = "app.packages.action.upgrade_package"
            }
            struct KegPolicy {
                static let useGlobal = "app.packages.keg_policy.use_global"
                static let keepOld = "app.packages.keg_policy.keep_old"
                static let cleanupOld = "app.packages.keg_policy.cleanup_old"
            }
        }

        struct Managers {
            struct Tab {
                static let title = "app.managers.tab.title"
            }
            struct State {
                static let enabled = "app.managers.state.enabled"
                static let disabled = "app.managers.state.disabled"
                static let notInstalled = "app.managers.state.not_installed"
                static let comingSoon = "app.managers.state.coming_soon"
            }
            struct Label {
                static let packageCount = "app.managers.label.package_count"
            }
            struct Name {
                static let homebrew = "app.managers.name.homebrew"
                static let homebrewCask = "app.managers.name.homebrew_cask"
                static let npm = "app.managers.name.npm"
                static let pnpm = "app.managers.name.pnpm"
                static let yarn = "app.managers.name.yarn"
                static let poetry = "app.managers.name.poetry"
                static let rubygems = "app.managers.name.rubygems"
                static let bundler = "app.managers.name.bundler"
                static let pip = "app.managers.name.pip"
                static let pipx = "app.managers.name.pipx"
                static let cargo = "app.managers.name.cargo"
                static let cargoBinstall = "app.managers.name.cargo_binstall"
                static let mise = "app.managers.name.mise"
                static let rustup = "app.managers.name.rustup"
                static let softwareUpdate = "app.managers.name.software_update"
                static let appStore = "app.managers.name.app_store"
            }
            struct Category {
                static let toolchain = "app.managers.category.toolchain"
                static let systemOs = "app.managers.category.system_os"
                static let language = "app.managers.category.language"
                static let appStore = "app.managers.category.app_store"
            }
            struct Help {
                static let enableDisable = "app.managers.help.enable_disable"
            }
            struct Tooltip {
                static let lastTaskFailed = "app.managers.tooltip.last_task_failed"
                static let outdatedWithUnknown = "app.managers.tooltip.outdated_with_unknown"
                static let outdated = "app.managers.tooltip.outdated"
                static let versionUnknown = "app.managers.tooltip.version_unknown"
                static let allUpToDate = "app.managers.tooltip.all_up_to_date"
            }
            struct Action {
                static let viewPackages = "app.managers.action.view_packages"
                static let install = "app.managers.action.install"
                static let update = "app.managers.action.update"
                static let uninstall = "app.managers.action.uninstall"
            }
            struct Alert {
                static let installTitle = "app.managers.alert.install.title"
                static let installMessage = "app.managers.alert.install.message"
                static let updateTitle = "app.managers.alert.update.title"
                static let updateMessage = "app.managers.alert.update.message"
                static let uninstallTitle = "app.managers.alert.uninstall.title"
                static let uninstallMessage = "app.managers.alert.uninstall.message"
            }
            struct Operation {
                static let startingInstall = "app.managers.operation.starting_install"
                static let startingUpdate = "app.managers.operation.starting_update"
                static let startingUninstall = "app.managers.operation.starting_uninstall"
                static let installFailed = "app.managers.operation.install_failed"
                static let updateFailed = "app.managers.operation.update_failed"
                static let uninstallFailed = "app.managers.operation.uninstall_failed"
                static let installing = "app.managers.operation.installing"
                static let updating = "app.managers.operation.updating"
                static let uninstalling = "app.managers.operation.uninstalling"
                static let upgrading = "app.managers.operation.upgrading"
            }
        }

        struct Onboarding {
            struct Welcome {
                static let title = "app.onboarding.welcome.title"
                static let subtitle = "app.onboarding.welcome.subtitle"
                static let getStarted = "app.onboarding.welcome.action.get_started"
            }
            struct Detection {
                static let title = "app.onboarding.detection.title"
                static let noneDetected = "app.onboarding.detection.none_detected"
                static let foundCount = "app.onboarding.detection.found_count"
                static let scanning = "app.onboarding.detection.scanning"
                static let `continue` = "app.onboarding.detection.action.continue"
            }
            struct Configure {
                static let title = "app.onboarding.configure.title"
                static let subtitle = "app.onboarding.configure.subtitle"
                static let noneDetected = "app.onboarding.configure.none_detected"
                static let installLater = "app.onboarding.configure.install_later"
                static let finishSetup = "app.onboarding.configure.action.finish_setup"
            }
            struct Settings {
                static let title = "app.onboarding.settings.title"
                static let subtitle = "app.onboarding.settings.subtitle"
                static let safeModeDescription = "app.onboarding.settings.safe_mode_description"
                static let autoCleanDescription = "app.onboarding.settings.auto_clean_description"
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
                static let systemDefaultWithEnglish = "app.settings.label.language.system_default_with_english"
                static let english = "app.settings.label.language.english"
                static let spanish = "app.settings.label.language.spanish"
                static let german = "app.settings.label.language.german"
                static let french = "app.settings.label.language.french"
                static let portugueseBrazilian = "app.settings.label.language.portuguese_brazilian"
                static let japanese = "app.settings.label.language.japanese"
                static let autoCheck = "app.settings.label.auto_check"
                static let checkFrequency = "app.settings.label.check_frequency"
                static let safeMode = "app.settings.label.safe_mode"
                static let autoCleanKegs = "app.settings.label.auto_clean_kegs"
            }
            struct Frequency {
                static let every15Min = "app.settings.frequency.every_15_min"
                static let every30Min = "app.settings.frequency.every_30_min"
                static let every1Hour = "app.settings.frequency.every_1_hour"
                static let daily = "app.settings.frequency.daily"
            }
            struct Action {
                static let refreshNow = "app.settings.action.refresh_now"
                static let upgradeAll = "app.settings.action.upgrade_all"
                static let reset = "app.settings.action.reset"
                static let quit = "app.settings.action.quit"
                static let replayWalkthrough = "app.settings.action.replay_walkthrough"
            }
            struct Metric {
                static let managers = "app.settings.metric.managers"
                static let updates = "app.settings.metric.updates"
                static let tasks = "app.settings.metric.tasks"
            }
            struct Alert {
                struct Reset {
                    static let title = "app.settings.alert.reset.title"
                    static let message = "app.settings.alert.reset.message"
                }
                struct UpgradeAll {
                    static let title = "app.settings.alert.upgrade_all.title"
                    static let upgradeNoOs = "app.settings.alert.upgrade_all.upgrade_no_os"
                    static let upgradeWithOs = "app.settings.alert.upgrade_all.upgrade_with_os"
                    static let safeModeMessage = "app.settings.alert.upgrade_all.safe_mode_message"
                    static let standardMessage = "app.settings.alert.upgrade_all.standard_message"
                    static let dryRunToggle = "app.settings.alert.upgrade_all.dry_run_toggle"
                    static let dryRunResultTitle = "app.settings.alert.upgrade_all.dry_run_result_title"
                    static let dryRunResultMessage = "app.settings.alert.upgrade_all.dry_run_result_message"
                }
            }
        }

        struct Tasks {
            static let noRecentTasks = "app.tasks.no_recent_tasks"
            static let fallbackDescription = "app.tasks.fallback.description"
            static let cancelUnavailable = "app.tasks.help.cancel_unavailable"

            struct Action {
                static let cancel = "app.tasks.action.cancel"
            }
        }

        struct Window {
            static let controlCenter = "app.window.control_center"
        }
        struct Section {
            static let updates = "app.section.updates"
            static let tasks = "app.section.tasks"
        }
        struct Health {
            static let healthy = "app.health.healthy"
            static let attention = "app.health.attention"
            static let error = "app.health.error"
            static let running = "app.health.running"
            static let notInstalled = "app.health.not_installed"
        }
        struct Popover {
            static let systemHealth = "app.popover.system_health"
            static let pendingUpdates = "app.popover.pending_updates"
            static let failures = "app.popover.failures"
            static let runningTasks = "app.popover.running_tasks"
            static let managerSnapshot = "app.popover.manager_snapshot"
            static let activeTasks = "app.popover.active_tasks"
            static let searchPlaceholder = "app.popover.search_placeholder"
            static let version = "app.popover.version"
            struct Banner {
                static let disconnectedTitle = "app.popover.banner.disconnected.title"
                static let disconnectedMessage = "app.popover.banner.disconnected.message"
                static let failedTitle = "app.popover.banner.failed.title"
                static let failedMessage = "app.popover.banner.failed.message"
                static let updatesTitle = "app.popover.banner.updates.title"
                static let updatesMessage = "app.popover.banner.updates.message"
            }
        }
        struct Action {
            static let openControlCenter = "app.action.open_control_center"
            static let refreshPlan = "app.action.refresh_plan"
            static let dryRun = "app.action.dry_run"
            static let runPlan = "app.action.run_plan"
        }
        struct Overview {
            static let managerHealth = "app.overview.manager_health"
            static let recentTasks = "app.overview.recent_tasks"
        }
        struct Updates {
            static let executionPlan = "app.updates.execution_plan"
            static let includeOs = "app.updates.include_os"
            static let managers = "app.updates.managers"
            static let packages = "app.updates.packages"
            static let riskFlags = "app.updates.risk_flags"
            struct Authority {
                static let authoritative = "app.updates.authority.authoritative"
                static let standard = "app.updates.authority.standard"
                static let guarded = "app.updates.authority.guarded"
            }
            struct Risk {
                static let privileged = "app.updates.risk.privileged"
                static let reboot = "app.updates.risk.reboot"
            }
        }
        struct DryRun {
            static let title = "app.dry_run.title"
            static let message = "app.dry_run.message"
        }
        struct Inspector {
            static let title = "app.inspector.title"
            static let empty = "app.inspector.empty"
            static let manager = "app.inspector.manager"
            static let installed = "app.inspector.installed"
            static let latest = "app.inspector.latest"
            static let sourceQuery = "app.inspector.source_query"
            static let capabilities = "app.inspector.capabilities"
            static let taskId = "app.inspector.task_id"
            static let taskType = "app.inspector.task_type"
            static let taskStatus = "app.inspector.task_status"
            static let taskManager = "app.inspector.task_manager"
            static let taskLabelKey = "app.inspector.task_label_key"
            static let taskLabelArgs = "app.inspector.task_label_args"
            static let packageId = "app.inspector.package_id"
            static let packageStatus = "app.inspector.package_status"
            static let pinned = "app.inspector.pinned"
            static let restartRequired = "app.inspector.restart_required"
            static let detected = "app.inspector.detected"
            static let notDetected = "app.inspector.not_detected"
            static let version = "app.inspector.version"
            static let executablePath = "app.inspector.executable_path"
            static let enabled = "app.inspector.enabled"
            static let disabled = "app.inspector.disabled"
            static let category = "app.inspector.category"
            static let installMethod = "app.inspector.install_method"
            struct InstallMethod {
                static let automatable = "app.inspector.install_method.automatable"
                static let updateAndUninstall = "app.inspector.install_method.update_and_uninstall"
                static let updateOnly = "app.inspector.install_method.update_only"
                static let systemBinary = "app.inspector.install_method.system_binary"
                static let notManageable = "app.inspector.install_method.not_manageable"
            }
        }
        struct Overlay {
            struct Search {
                static let title = "app.overlay.search.title"
                static let empty = "app.overlay.search.empty"
                static let openPackages = "app.overlay.search.open_packages"
            }
            struct Settings {
                static let title = "app.overlay.settings.title"
                static let openAdvanced = "app.overlay.settings.open_advanced"
            }
            struct About {
                static let title = "app.overlay.about.title"
                static let name = "app.overlay.about.name"
                static let subtitle = "app.overlay.about.subtitle"
                static let version = "app.overlay.about.version"
                static let summary = "app.overlay.about.summary"
            }
            struct Quit {
                static let title = "app.overlay.quit.title"
                static let message = "app.overlay.quit.message"
            }
        }
        struct ControlCenter {
            static let searchPlaceholder = "app.control_center.search_placeholder"
            static let upgradeAll = "app.control_center.upgrade_all"
        }
        struct TasksSection {
            static let empty = "app.tasks.empty"
        }
        struct ManagersSection {
            static let empty = "app.managers.empty"
        }
        struct Capability {
            static let list = "app.capability.list"
            static let outdated = "app.capability.outdated"
            static let search = "app.capability.search"
            static let install = "app.capability.install"
            static let uninstall = "app.capability.uninstall"
            static let upgrade = "app.capability.upgrade"
            static let pin = "app.capability.pin"
        }
    }
}

extension L10n.App {
    struct Walkthrough {
        struct Control {
            static let next = "app.walkthrough.control.next"
            static let skip = "app.walkthrough.control.skip"
            static let done = "app.walkthrough.control.done"
            static let stepIndicator = "app.walkthrough.control.step_indicator"
        }
        struct Popover {
            struct Step1 {
                static let title = "app.walkthrough.popover.step1.title"
                static let description = "app.walkthrough.popover.step1.description"
            }
            struct Step2 {
                static let title = "app.walkthrough.popover.step2.title"
                static let description = "app.walkthrough.popover.step2.description"
            }
            struct Step3 {
                static let title = "app.walkthrough.popover.step3.title"
                static let description = "app.walkthrough.popover.step3.description"
            }
            struct Step4 {
                static let title = "app.walkthrough.popover.step4.title"
                static let description = "app.walkthrough.popover.step4.description"
            }
            struct Step5 {
                static let title = "app.walkthrough.popover.step5.title"
                static let description = "app.walkthrough.popover.step5.description"
            }
            struct Step6 {
                static let title = "app.walkthrough.popover.step6.title"
                static let description = "app.walkthrough.popover.step6.description"
            }
        }
        struct ControlCenter {
            struct Step1 {
                static let title = "app.walkthrough.control_center.step1.title"
                static let description = "app.walkthrough.control_center.step1.description"
            }
            struct Step2 {
                static let title = "app.walkthrough.control_center.step2.title"
                static let description = "app.walkthrough.control_center.step2.description"
            }
            struct Step3 {
                static let title = "app.walkthrough.control_center.step3.title"
                static let description = "app.walkthrough.control_center.step3.description"
            }
            struct Step4 {
                static let title = "app.walkthrough.control_center.step4.title"
                static let description = "app.walkthrough.control_center.step4.description"
            }
            struct Step5 {
                static let title = "app.walkthrough.control_center.step5.title"
                static let description = "app.walkthrough.control_center.step5.description"
            }
            struct Step6 {
                static let title = "app.walkthrough.control_center.step6.title"
                static let description = "app.walkthrough.control_center.step6.description"
            }
            struct Step7 {
                static let title = "app.walkthrough.control_center.step7.title"
                static let description = "app.walkthrough.control_center.step7.description"
            }
        }
    }
}
