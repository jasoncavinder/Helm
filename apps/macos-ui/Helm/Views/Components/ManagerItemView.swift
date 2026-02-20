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
        if hasFailedTasks { return HelmTheme.stateError }
        if hasOutdatedPackages || !versionAvailable { return HelmTheme.stateAttention }
        return HelmTheme.stateHealthy
    }

    private var statusTooltip: String {
        if hasFailedTasks { return L10n.App.Managers.Tooltip.lastTaskFailed.localized }
        if hasOutdatedPackages && !versionAvailable {
            return L10n.App.Managers.Tooltip.outdatedWithUnknown.localized(with: ["count": outdatedCount])
        }
        if hasOutdatedPackages { return L10n.App.Managers.Tooltip.outdated.localized(with: ["count": outdatedCount]) }
        if !versionAvailable { return L10n.App.Managers.Tooltip.versionUnknown.localized }
        return L10n.App.Managers.Tooltip.allUpToDate.localized
    }

    private var accessibilityDescription: String {
        var parts = [manager.displayName]
        parts.append(statusTooltip)
        if packageCount > 0 {
            parts.append("\(packageCount)")
        }
        return parts.joined(separator: ", ")
    }

    var body: some View {
        VStack(spacing: 4) {
            ZStack(alignment: .topTrailing) {
                RoundedRectangle(cornerRadius: 8)
                    .fill(HelmTheme.selectionFill)
                    .frame(width: 44, height: 44)
                    .overlay(
                        Text(manager.firstLetter)
                            .font(.title2)
                            .fontWeight(.bold)
                            .foregroundColor(HelmTheme.actionPrimaryDefault)
                    )

                Circle()
                    .fill(indicatorColor)
                    .frame(width: 8, height: 8)
                    .offset(x: 2, y: -2)
            }

            Text(manager.shortName)
                .font(.caption2)
                .foregroundColor(HelmTheme.textPrimary)
                .lineLimit(1)

            if packageCount > 0 {
                Text("\(packageCount)")
                    .font(.caption2)
                    .foregroundColor(HelmTheme.textSecondary)
            }
        }
        .frame(width: 60)
        .contentShape(Rectangle())
        .help(statusTooltip)
        .onTapGesture { onTap() }
        .contextMenu {
            Button(L10n.App.Managers.Action.viewPackages.localized) { onTap() }
            Button(L10n.Common.refresh.localized) { onRefresh() }
            Divider()
            Text(statusTooltip)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityDescription)
        .accessibilityAddTraits(.isButton)
    }
}
