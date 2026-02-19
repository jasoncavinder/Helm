import SwiftUI

struct TaskItem: Identifiable {
    let id: String
    let description: String
    let status: String
    let managerId: String?
    let taskType: String?
    let labelKey: String?
    let labelArgs: [String: String]?

    var isRunning: Bool {
        let s = status.lowercased()
        return s == "running" || s == "queued"
    }

    /// Sort order: running first, then queued, then terminal states.
    // swiftlint:disable:next cyclomatic_complexity
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
        case "cancelled": return .orange
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
