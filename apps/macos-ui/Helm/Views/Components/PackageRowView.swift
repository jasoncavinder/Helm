import SwiftUI

enum KegPolicyMenuSelection {
    case useGlobal
    case keep
    case cleanup
}

struct PackageRowView: View {
    let package: PackageItem
    var managerDisplayNames: [String]?
    var isSelected: Bool = false
    var isPinActionInFlight: Bool = false
    var isUpgradeActionInFlight: Bool = false
    var isInstallActionInFlight: Bool = false
    var kegPolicySelection: KegPolicyMenuSelection?
    var onSelectKegPolicy: ((KegPolicyMenuSelection) -> Void)?
    var onUpgrade: (() -> Void)?
    var onInstall: (() -> Void)?
    var onTogglePin: (() -> Void)?

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
            if let onUpgrade, package.status == .upgradable {
                Button(action: onUpgrade) {
                    Image(systemName: package.status.iconName)
                        .foregroundColor(package.status.iconColor)
                        .font(.body)
                        .frame(width: 20)
                }
                .buttonStyle(.borderless)
                .disabled(isUpgradeActionInFlight)
                .help(L10n.App.Packages.Action.upgradePackage.localized)
                .helmPointer(enabled: !isUpgradeActionInFlight)
                .accessibilityHidden(true)
            } else if let onInstall, package.status == .available {
                Button(action: onInstall) {
                    Image(systemName: package.status.iconName)
                        .foregroundColor(package.status.iconColor)
                        .font(.body)
                        .frame(width: 20)
                }
                .buttonStyle(.borderless)
                .disabled(isInstallActionInFlight)
                .help(L10n.App.Packages.Action.install.localized)
                .helmPointer(enabled: !isInstallActionInFlight)
                .accessibilityHidden(true)
            } else {
                Image(systemName: package.status.iconName)
                    .foregroundColor(package.status.iconColor)
                    .font(.body)
                    .frame(width: 20)
                    .accessibilityHidden(true)
            }

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

                if displayedManagerNames.count == 1 {
                    Text(displayedManagerNames[0])
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
                } else {
                    Text(displayedManagerNames.joined(separator: ", "))
                        .font(.caption2)
                        .foregroundColor(HelmTheme.textSecondary)
                        .lineLimit(2)
                }
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 2) {
                HStack(spacing: 6) {
                    if let onSelectKegPolicy, let kegPolicySelection {
                        Menu {
                            Button {
                                onSelectKegPolicy(.useGlobal)
                            } label: {
                                HStack {
                                    Text(L10n.App.Packages.KegPolicy.useGlobal.localized)
                                    if kegPolicySelection == .useGlobal {
                                        Image(systemName: "checkmark")
                                    }
                                }
                            }

                            Button {
                                onSelectKegPolicy(.keep)
                            } label: {
                                HStack {
                                    Text(L10n.App.Packages.KegPolicy.keepOld.localized)
                                    if kegPolicySelection == .keep {
                                        Image(systemName: "checkmark")
                                    }
                                }
                            }

                            Button {
                                onSelectKegPolicy(.cleanup)
                            } label: {
                                HStack {
                                    Text(L10n.App.Packages.KegPolicy.cleanupOld.localized)
                                    if kegPolicySelection == .cleanup {
                                        Image(systemName: "checkmark")
                                    }
                                }
                            }
                        } label: {
                            Image(systemName: "shippingbox")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                        .menuStyle(.borderlessButton)
                        .help(L10n.App.Packages.Label.homebrewKegPolicy.localized)
                        .helmPointer()
                    }

                    if let onUpgrade {
                        if isUpgradeActionInFlight {
                            ProgressView()
                                .controlSize(.mini)
                                .scaleEffect(0.75)
                                .frame(width: 16, height: 16)
                        } else {
                            Button(action: onUpgrade) {
                                Image(systemName: "arrow.up.circle")
                                    .font(.caption)
                                    .foregroundColor(HelmTheme.stateAttention)
                            }
                            .buttonStyle(.borderless)
                            .help(L10n.App.Packages.Action.upgradePackage.localized)
                            .helmPointer()
                        }
                    }

                    if let onTogglePin {
                        if isPinActionInFlight {
                            ProgressView()
                                .controlSize(.mini)
                                .scaleEffect(0.75)
                                .frame(width: 16, height: 16)
                        } else {
                            Button(action: onTogglePin) {
                                Image(systemName: package.pinned ? "pin.slash.fill" : "pin")
                                    .font(.caption)
                                    .foregroundColor(package.pinned ? HelmTheme.stateAttention : HelmTheme.textSecondary)
                            }
                            .buttonStyle(.borderless)
                            .help(
                                package.pinned
                                    ? L10n.App.Packages.Action.unpin.localized
                                    : L10n.App.Packages.Action.pin.localized
                            )
                            .helmPointer()
                        }
                    }
                }

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
        .padding(.vertical, 4)
        .padding(.horizontal, 8)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(isSelected ? HelmTheme.selectionFill : Color.clear)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .stroke(isSelected ? HelmTheme.selectionStroke : Color.clear, lineWidth: 1)
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityDescription)
    }
}
