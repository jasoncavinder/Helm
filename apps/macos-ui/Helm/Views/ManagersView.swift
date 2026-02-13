import SwiftUI

struct ManagersView: View {
    @ObservedObject var core = HelmCore.shared
    @Binding var selectedTab: HelmTab

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                ForEach(ManagerInfo.all) { manager in
                    let status = core.managerStatuses[manager.id]
                    let detected = status?.detected ?? core.detectedManagers.contains(manager.id)
                    let enabled = status?.enabled ?? true
                    let packageCount = countFor(manager: manager)

                    ManagerRow(
                        manager: manager,
                        detected: detected,
                        enabled: enabled,
                        version: status?.version,
                        packageCount: packageCount,
                        onToggle: {
                            core.setManagerEnabled(manager.id, enabled: !enabled)
                        },
                        onTap: {
                            core.selectedManagerFilter = normalizedManagerName(manager.id)
                            selectedTab = .packages
                        },
                        onInstall: {
                            core.installManager(manager.id)
                        },
                        onUninstall: {
                            core.uninstallManager(manager.id)
                        }
                    )

                    Divider()
                        .padding(.leading, 44)
                }
            }
        }
    }

    private func countFor(manager: ManagerInfo) -> Int {
        core.installedPackages.filter {
            $0.manager.lowercased().contains(manager.shortName.lowercased())
        }.count
    }

    private func normalizedManagerName(_ raw: String) -> String {
        switch raw.lowercased() {
        case "homebrew_formula": return "Homebrew"
        case "homebrew_cask": return "Homebrew Cask"
        case "npm_global": return "npm"
        case "pipx": return "pipx"
        case "cargo": return "Cargo"
        case "mise": return "mise"
        case "rustup": return "rustup"
        default: return raw.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }
}

private struct ManagerRow: View {
    let manager: ManagerInfo
    let detected: Bool
    let enabled: Bool
    let version: String?
    let packageCount: Int
    let onToggle: () -> Void
    let onTap: () -> Void
    let onInstall: () -> Void
    let onUninstall: () -> Void

    private var indicatorColor: Color {
        if !manager.isImplemented { return .gray }
        if !enabled { return .gray }
        return detected ? .green : .red
    }

    var body: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(indicatorColor)
                .frame(width: 8, height: 8)

            VStack(alignment: .leading, spacing: 1) {
                Text(manager.displayName)
                    .font(.subheadline)
                    .fontWeight(.medium)
                    .foregroundColor(manager.isImplemented ? .primary : .secondary)

                HStack(spacing: 6) {
                    Text(manager.category)
                        .font(.caption2)
                        .foregroundColor(.secondary)

                    if let version = version {
                        Text("v\(version)")
                            .font(.caption2)
                            .foregroundColor(.secondary)
                    }

                    if packageCount > 0 {
                        Text("\(packageCount) pkg\(packageCount == 1 ? "" : "s")")
                            .font(.caption2)
                            .foregroundColor(.blue)
                    }
                }
            }

            Spacer()

            if manager.isImplemented {
                Toggle("", isOn: Binding(
                    get: { enabled },
                    set: { _ in onToggle() }
                ))
                .toggleStyle(.switch)
                .scaleEffect(0.7)
                .labelsHidden()
            } else {
                Text("Coming Soon")
                    .font(.caption2)
                    .foregroundColor(.secondary)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Color.gray.opacity(0.1))
                    .cornerRadius(4)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .contentShape(Rectangle())
        .onTapGesture {
            if manager.isImplemented && detected && packageCount > 0 {
                onTap()
            }
        }
        .contextMenu {
            if manager.canInstall && !detected {
                Button("Install \(manager.shortName)") {
                    onInstall()
                }
            }
            if manager.canUninstall && detected {
                Button("Uninstall \(manager.shortName)") {
                    onUninstall()
                }
            }
            if manager.isImplemented && detected && packageCount > 0 {
                Button("View Packages") {
                    onTap()
                }
            }
        }
        .opacity(manager.isImplemented ? 1.0 : 0.6)
    }
}
