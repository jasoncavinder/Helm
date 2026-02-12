import SwiftUI

struct TaskItem: Identifiable {
    let id: String
    let description: String
    let status: String

    var isRunning: Bool {
        let s = status.lowercased()
        return s == "running" || s == "queued"
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
}
