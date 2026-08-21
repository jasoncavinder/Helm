import SwiftUI

struct TaskItem: Identifiable {
    let id: String
    let description: String
    let status: String
    let managerId: String?
    let taskType: String?
    let labelKey: String?
    let labelArgs: [String: String]?
    let fallbackLocalization: TaskDescriptionLocalization?

    init(
        id: String,
        description: String,
        status: String,
        managerId: String?,
        taskType: String?,
        labelKey: String?,
        labelArgs: [String: String]?,
        fallbackLocalization: TaskDescriptionLocalization? = nil
    ) {
        self.id = id
        self.description = description
        self.status = status
        self.managerId = managerId
        self.taskType = taskType
        self.labelKey = labelKey
        self.labelArgs = labelArgs
        self.fallbackLocalization = fallbackLocalization
    }

    var isRunning: Bool {
        let s = status.lowercased()
        return s == "running" || s == "queued"
    }

    var isFailed: Bool {
        status.lowercased() == "failed"
    }

    var supportsInlineDetails: Bool {
        true
    }

    var localizedDescription: String {
        localizedDescription { key, arguments in
            let localizedArguments = arguments.reduce(into: [String: Any]()) { result, entry in
                result[entry.key] = entry.value
            }
            return LocalizationManager.shared.stringIfPresent(key, args: localizedArguments)
        } argumentResolver: { argument in
            switch argument {
            case let .literal(value):
                return value
            case let .managerID(managerID):
                return localizedManagerDisplayName(managerID)
            case let .taskType(taskType):
                return HelmCore.shared.localizedTaskType(taskType)
            }
        }
    }

    func localizedDescription(
        using resolver: (_ key: String, _ arguments: [String: String]) -> String?,
        argumentResolver: (TaskDescriptionLocalization.Argument) -> String = { $0.rawValue }
    ) -> String {
        TaskDescriptionPresentation(
            rawDescription: description,
            labelKey: labelKey,
            labelArgs: labelArgs,
            fallbackLocalization: fallbackLocalization
        ).resolve(using: resolver, argumentResolver: argumentResolver)
    }

    /// Sort order: running first, then queued, then terminal states.
    var statusSortOrder: Int {
        switch status.lowercased() {
        case "running":   return 0
        case "queued":    return 1
        case "failed":    return 2
        case "cancelled": return 3
        case "completed": return 4
        default:          return 5
        }
    }

    var statusIcon: String {
        switch status.lowercased() {
        case "running":   return "arrow.triangle.2.circlepath"
        case "queued":    return "clock"
        case "completed": return "checkmark.circle.fill"
        case "failed":    return "xmark.circle.fill"
        case "cancelled": return "minus.circle.fill"
        default:          return "questionmark.circle"
        }
    }

    var statusColor: Color {
        switch status.lowercased() {
        case "running":   return .blue
        case "queued":    return .secondary
        case "completed": return .green
        case "failed":    return .red
        case "cancelled": return HelmTheme.stateUnavailable
        default:          return .secondary
        }
    }
    
    var localizedStatus: String {
        switch status.lowercased() {
        case "queued": return L10n.Service.Task.Status.pending.localized
        case "running": return L10n.Service.Task.Status.running.localized
        case "completed": return L10n.Service.Task.Status.completed.localized
        case "failed": return L10n.Service.Task.Status.failed.localized
        case "cancelled": return L10n.Service.Task.Status.cancelled.localized
        default: return status
        }
    }
}
