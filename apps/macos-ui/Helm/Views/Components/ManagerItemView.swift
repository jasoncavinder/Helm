import SwiftUI

struct ManagerItemView: View {
    let manager: ManagerInfo
    let packageCount: Int
    let hasOutdatedPackages: Bool
    let hasFailedTasks: Bool
    let versionAvailable: Bool
    let outdatedCount: Int
    let onTap: () -> Void
    let onRefresh: () -> Void

    private var indicatorColor: Color {
        if hasFailedTasks { return .red }
        if hasOutdatedPackages || !versionAvailable { return .yellow }
        return .green
    }

    private var statusTooltip: String {
        if hasFailedTasks { return "Last task failed" }
        if hasOutdatedPackages && !versionAvailable {
            return "\(outdatedCount) outdated \u{00B7} Version unknown"
        }
        if hasOutdatedPackages { return "\(outdatedCount) outdated" }
        if !versionAvailable { return "Version unknown" }
        return "All up to date"
    }

    var body: some View {
        VStack(spacing: 4) {
            ZStack(alignment: .topTrailing) {
                RoundedRectangle(cornerRadius: 8)
                    .fill(Color.accentColor.opacity(0.15))
                    .frame(width: 44, height: 44)
                    .overlay(
                        Text(manager.firstLetter)
                            .font(.title2)
                            .fontWeight(.bold)
                            .foregroundColor(.accentColor)
                    )

                Circle()
                    .fill(indicatorColor)
                    .frame(width: 8, height: 8)
                    .offset(x: 2, y: -2)
                    .help(statusTooltip)
            }

            Text(manager.shortName)
                .font(.caption2)
                .foregroundColor(.primary)
                .lineLimit(1)

            if packageCount > 0 {
                Text("\(packageCount)")
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }
        }
        .frame(width: 60)
        .contentShape(Rectangle())
        .onTapGesture { onTap() }
        .contextMenu {
            Button("View Packages") { onTap() }
            Button("Refresh") { onRefresh() }
            Divider()
            Text(statusTooltip)
        }
    }
}
