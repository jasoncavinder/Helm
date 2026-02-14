import SwiftUI

struct ManagersView: View {
    @ObservedObject var core = HelmCore.shared
    @Binding var selectedTab: HelmTab

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                ForEach(ManagerInfo.groupedByCategory, id: \.category) { group in
                    // Section header
                    HStack {
                        Text(group.category)
                            .font(.caption)
                            .fontWeight(.semibold)
                            .foregroundColor(.secondary)
                            .textCase(.uppercase)
                        Spacer()
                    }
                    .padding(.horizontal, 12)
                    .padding(.top, 10)
                    .padding(.bottom, 4)

                    ForEach(group.managers) { manager in
                        let status = core.managerStatuses[manager.id]
                        let detected = status?.detected ?? false
                        let enabled = status?.enabled ?? true
                        let packageCount = countFor(manager: manager)

                        ManagerRow(
                            manager: manager,
                            detected: detected,
                            enabled: enabled,
                            version: status?.version,
                            packageCount: packageCount,
                            operationStatus: core.managerOperations[manager.id],
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
                            onUpdate: {
                                core.updateManager(manager.id)
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
        case "softwareupdate": return "Software Update"
        case "mas": return "App Store"
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
    let operationStatus: String?
    let onToggle: () -> Void
    let onTap: () -> Void
    let onInstall: () -> Void
    let onUpdate: () -> Void
    let onUninstall: () -> Void

    @State private var confirmAction: ConfirmAction? = nil

    private enum ConfirmAction: Identifiable {
        case install, update, uninstall
        var id: String {
            switch self {
            case .install: return "install"
            case .update: return "update"
            case .uninstall: return "uninstall"
            }
        }
    }

    private var indicatorColor: Color {
        if !manager.isImplemented { return .gray }
        if !enabled { return .gray }
        return detected ? .green : .red
    }

    var body: some View {
        HStack(spacing: 10) {
            // Info area — tappable to navigate to packages
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
            }
            .contentShape(Rectangle())
            .onTapGesture {
                if manager.isImplemented && detected && packageCount > 0 {
                    onTap()
                }
            }

            if manager.isImplemented {
                if let operationStatus {
                    HStack(spacing: 4) {
                        ProgressView()
                            .scaleEffect(0.6)
                        Text(operationStatus)
                            .font(.caption2)
                            .foregroundColor(.secondary)
                    }
                }
                if detected {
                    Text(enabled ? "Enabled" : "Disabled")
                        .font(.caption2)
                        .foregroundColor(.secondary)

                    Toggle("", isOn: Binding(
                        get: { enabled },
                        set: { _ in onToggle() }
                    ))
                    .toggleStyle(.switch)
                    .scaleEffect(0.7)
                    .labelsHidden()
                    .help("Enable or disable this manager")
                } else {
                    Text("Not Installed")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
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
        .contextMenu {
            if manager.canInstall && !detected {
                Button("Install \(manager.shortName)") {
                    confirmAction = .install
                }
            }
            if manager.canUpdate && detected {
                Button("Update \(manager.shortName)") {
                    confirmAction = .update
                }
            }
            if manager.canUninstall && detected {
                Button("Uninstall \(manager.shortName)") {
                    confirmAction = .uninstall
                }
            }
            if manager.isImplemented && detected && packageCount > 0 {
                Button("View Packages") {
                    onTap()
                }
            }
        }
        .alert(item: $confirmAction) { action in
            switch action {
            case .install:
                return Alert(
                    title: Text("Install \(manager.displayName)?"),
                    message: Text("This will install \(manager.shortName) via Homebrew."),
                    primaryButton: .default(Text("Install")) { onInstall() },
                    secondaryButton: .cancel()
                )
            case .update:
                return Alert(
                    title: Text("Update \(manager.displayName)?"),
                    message: Text("This will run the manager's supported update flow."),
                    primaryButton: .default(Text("Update")) { onUpdate() },
                    secondaryButton: .cancel()
                )
            case .uninstall:
                return Alert(
                    title: Text("Uninstall \(manager.displayName)?"),
                    message: Text("This will remove \(manager.shortName) from your system."),
                    primaryButton: .destructive(Text("Uninstall")) { onUninstall() },
                    secondaryButton: .cancel()
                )
            }
        }
        .opacity(manager.isImplemented ? 1.0 : 0.6)
    }
}
