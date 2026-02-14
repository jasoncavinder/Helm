import SwiftUI

enum KegPolicyMenuSelection {
    case useGlobal
    case keep
    case cleanup
}

struct PackageRowView: View {
    let package: PackageItem
    var isPinActionInFlight: Bool = false
    var isUpgradeActionInFlight: Bool = false
    var kegPolicySelection: KegPolicyMenuSelection? = nil
    var onSelectKegPolicy: ((KegPolicyMenuSelection) -> Void)? = nil
    var onUpgrade: (() -> Void)? = nil
    var onTogglePin: (() -> Void)? = nil

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: package.status.iconName)
                .foregroundColor(package.status.iconColor)
                .font(.body)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(package.name)
                        .font(.body)
                        .lineLimit(1)
                    if package.pinned {
                        Image(systemName: "pin.fill")
                            .font(.caption2)
                            .foregroundColor(.secondary)
                            .help(L10n.App.Packages.Label.pinned.localized)
                    }
                }

                Text(package.manager)
                    .font(.caption2)
                    .padding(.horizontal, 4)
                    .padding(.vertical, 1)
                    .background(
                        RoundedRectangle(cornerRadius: 3)
                            .fill(Color.gray.opacity(0.15))
                    )
                    .foregroundColor(.secondary)
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
                                    .foregroundColor(.orange)
                            }
                            .buttonStyle(.borderless)
                            .help(L10n.App.Packages.Action.upgradePackage.localized)
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
                                    .foregroundColor(package.pinned ? .orange : .secondary)
                            }
                            .buttonStyle(.borderless)
                            .help(
                                package.pinned
                                    ? L10n.App.Packages.Action.unpin.localized
                                    : L10n.App.Packages.Action.pin.localized
                            )
                        }
                    }
                }

                if let latest = package.latestVersion {
                    HStack(spacing: 4) {
                        Text(latest)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundColor(.orange)
                            if package.restartRequired {
                                Image(systemName: "arrow.triangle.2.circlepath")
                                    .font(.caption2)
                                    .foregroundColor(.orange)
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
    }
}
