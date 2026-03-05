import SwiftUI

struct PackageRowView: View {
    let package: PackageItem
    var managerDisplayNames: [String]?
    var isSelected: Bool = false

    private var accessibilityDescription: String {
        let managerList = displayedManagerNames.joined(separator: ", ")
        var parts = [package.name, package.status.displayName]
        parts.append(package.version)
        if let latest = package.latestVersion {
            parts.append(L10n.App.Packages.Action.upgradePackage.localized + " " + latest)
        }
        if package.pinned {
            parts.append(L10n.App.Packages.Label.pinned.localized)
        }
        if package.restartRequired {
            parts.append(L10n.App.Packages.Label.restartRequired.localized)
        }
        parts.append(managerList)
        return parts.joined(separator: ", ")
    }

    private var displayedManagerNames: [String] {
        let names = managerDisplayNames?.filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } ?? []
        if names.isEmpty {
            return [package.manager]
        }
        return names
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: package.status.iconName)
                .foregroundColor(package.status.iconColor)
                .font(.body)
                .frame(width: 20)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(package.name)
                        .font(.body)
                        .lineLimit(1)
                    if package.pinned {
                        Image(systemName: "pin.fill")
                            .font(.caption2)
                            .foregroundColor(HelmTheme.stateAttention)
                            .help(L10n.App.Packages.Label.pinned.localized)
                            .accessibilityHidden(true)
                    }
                }

                HStack(spacing: 4) {
                    ForEach(Array(displayedManagerNames.enumerated()), id: \.offset) { _, managerName in
                        self.managerBadge(managerName)
                    }
                }
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 2) {
                if let latest = package.latestVersion {
                    HStack(spacing: 4) {
                        Text(latest)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundColor(HelmTheme.stateAttention)
                        if package.restartRequired {
                            Image(systemName: "arrow.triangle.2.circlepath")
                                .font(.caption2)
                                .foregroundColor(HelmTheme.stateAttention)
                                .help(L10n.App.Packages.Label.restartRequired.localized)
                        }
                    }
                    Text(package.version)
                        .font(.system(.caption2, design: .monospaced))
                        .foregroundColor(.secondary)
                        .strikethrough()
                } else {
                    Text(package.version)
                        .font(.system(.caption, design: .monospaced))
                }
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(isSelected ? HelmTheme.selectionFill : HelmTheme.surfaceElevated)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .stroke(
                    isSelected ? HelmTheme.selectionStroke : HelmTheme.borderSubtle.opacity(0.9),
                    lineWidth: 0.8
                )
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityDescription)
    }

    private func managerBadge(_ text: String) -> some View {
        Text(text)
            .font(.caption2)
            .padding(.horizontal, 4)
            .padding(.vertical, 1)
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(HelmTheme.surfaceElevated)
                    .overlay(
                        RoundedRectangle(cornerRadius: 4, style: .continuous)
                            .strokeBorder(HelmTheme.borderSubtle.opacity(0.9), lineWidth: 0.8)
                    )
            )
            .foregroundColor(HelmTheme.textSecondary)
    }
}
