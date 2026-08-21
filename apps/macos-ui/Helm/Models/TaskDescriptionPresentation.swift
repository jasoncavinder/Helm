import Foundation

struct TaskDescriptionLocalization: Equatable {
    enum Argument: Equatable {
        case literal(String)
        case managerID(String)
        case taskType(String)

        var rawValue: String {
            switch self {
            case let .literal(value), let .managerID(value), let .taskType(value):
                return value
            }
        }
    }

    let key: String
    let arguments: [String: Argument]

    static func genericTask(taskType: String, managerID: String) -> Self {
        Self(
            key: "app.tasks.fallback.description",
            arguments: [
                "task_type": .taskType(taskType),
                "manager": .managerID(managerID),
            ]
        )
    }

    static func productionTask(
        taskType: String,
        managerID: String,
        override: Self?
    ) -> Self {
        override ?? genericTask(taskType: taskType, managerID: managerID)
    }

    static func managerAction(taskType: String, managerID: String) -> Self? {
        switch taskType.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "install", "manager_install":
            return genericTask(taskType: "install", managerID: managerID)
        case "update", "upgrade", "manager_update":
            switch managerID {
            case "homebrew_formula":
                return Self(key: "service.task.label.update.homebrew_self", arguments: [:])
            case "rustup":
                return Self(key: "service.task.label.update.rustup_self", arguments: [:])
            default:
                return genericTask(taskType: "upgrade", managerID: managerID)
            }
        case "uninstall", "manager_uninstall":
            if managerID == "rustup" {
                return Self(key: "service.task.label.uninstall.rustup_self", arguments: [:])
            }
            return genericTask(taskType: "uninstall", managerID: managerID)
        case "manager_setup":
            return Self(
                key: "service.task.label.setup.manager",
                arguments: ["manager": .managerID(managerID)]
            )
        default:
            return nil
        }
    }

    static func packageUpgrade(
        packageName: String,
        managerID: String,
        cleanupOldKegs: Bool
    ) -> Self {
        switch managerID {
        case "homebrew_formula":
            return Self(
                key: cleanupOldKegs
                    ? "service.task.label.upgrade.homebrew_cleanup"
                    : "service.task.label.upgrade.homebrew",
                arguments: ["package": .literal(packageName)]
            )
        case "mise":
            return Self(
                key: "service.task.label.upgrade.mise",
                arguments: ["package": .literal(packageName)]
            )
        case "rustup":
            return Self(
                key: "service.task.label.upgrade.rustup_toolchain",
                arguments: ["toolchain": .literal(packageName)]
            )
        default:
            return Self(
                key: "service.task.label.upgrade.package",
                arguments: [
                    "package": .literal(packageName),
                    "manager": .managerID(managerID),
                ]
            )
        }
    }
}

struct TaskDescriptionPresentation {
    let rawDescription: String
    let labelKey: String?
    let labelArgs: [String: String]?
    let fallbackLocalization: TaskDescriptionLocalization?

    init(
        rawDescription: String,
        labelKey: String?,
        labelArgs: [String: String]?,
        fallbackLocalization: TaskDescriptionLocalization? = nil
    ) {
        self.rawDescription = rawDescription
        self.labelKey = labelKey
        self.labelArgs = labelArgs
        self.fallbackLocalization = fallbackLocalization
    }

    func resolve(
        using resolver: (_ key: String, _ arguments: [String: String]) -> String?,
        argumentResolver: (TaskDescriptionLocalization.Argument) -> String = { $0.rawValue }
    ) -> String {
        if let labelKey {
            let normalizedKey = labelKey.trimmingCharacters(in: .whitespacesAndNewlines)
            if !normalizedKey.isEmpty {
                if let localized = resolver(normalizedKey, labelArgs ?? [:]) {
                    return localized
                }
            }
        }

        guard let fallbackLocalization else {
            return rawDescription
        }
        let arguments = fallbackLocalization.arguments.mapValues(argumentResolver)
        return resolver(fallbackLocalization.key, arguments) ?? rawDescription
    }
}
