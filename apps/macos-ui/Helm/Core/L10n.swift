import Foundation

struct L10n {
    struct Common {
        static let ok = "common.button.ok"
        static let cancel = "common.button.cancel"
        static let `continue` = "common.button.continue"
        static let refresh = "common.button.refresh"
        static let install = "common.button.install"
        static let uninstall = "common.button.uninstall"
        static let update = "common.button.update"
        static let settings = "common.button.settings"
        static let quit = "common.button.quit"
        static let done = "common.button.done"
        static let reset = "common.button.reset"
        static let clear = "common.button.clear"
        static let close = "common.button.close"

        static let version = "common.label.version"
        static let loading = "common.label.loading"
        static let initializing = "common.label.initializing"
        static let error = "common.label.error"
        static let warning = "common.label.warning"
        static let success = "common.label.success"
        static let unknown = "common.label.unknown"
        static let detected = "common.label.detected"
        static let enabled = "common.label.enabled"
        static let disabled = "common.label.disabled"
        static let notInstalled = "common.label.not_installed"
        static let comingSoon = "common.label.coming_soon"
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
            struct Label {
                static let upgradeHomebrew = "service.task.label.upgrade.homebrew"
                static let upgradeHomebrewCleanup = "service.task.label.upgrade.homebrew_cleanup"
                static let upgradeMise = "service.task.label.upgrade.mise"
                static let upgradePackage = "service.task.label.upgrade.package"
                static let upgradeRustupToolchain = "service.task.label.upgrade.rustup_toolchain"
                static let upgradeSoftwareUpdateAll = "service.task.label.upgrade.softwareupdate_all"
                static let pinHomebrew = "service.task.label.pin.homebrew"
                static let unpinHomebrew = "service.task.label.unpin.homebrew"
                static let installHomebrewFormula = "service.task.label.install.homebrew_formula"
                static let updateHomebrewSelf = "service.task.label.update.homebrew_self"
                static let updateHomebrewFormula = "service.task.label.update.homebrew_formula"
                static let updateHomebrewFormulaCleanup = "service.task.label.update.homebrew_formula_cleanup"
                static let updateRustupSelf = "service.task.label.update.rustup_self"
                static let uninstallHomebrewFormula = "service.task.label.uninstall.homebrew_formula"
                static let uninstallRustupSelf = "service.task.label.uninstall.rustup_self"
            }
        }
    }
}
