import SwiftUI

struct DashboardView: View {
    @ObservedObject var core = HelmCore.shared
    @Binding var selectedTab: HelmTab

    var body: some View {
        VStack(spacing: 12) {
            // App icon + version | Stats
            HStack(alignment: .center, spacing: 12) {
                Image(nsImage: NSApp.applicationIconImage)
                    .resizable()
                    .frame(width: 36, height: 36)
                    .cornerRadius(8)

                VStack(alignment: .leading, spacing: 1) {
                    Text(L10n.App.Dashboard.title.localized)
                        .font(.headline)

                    Text(L10n.Common.version.localized(with: ["version": helmVersion]))
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }

                Spacer()

                VStack(alignment: .trailing, spacing: 4) {
                    StatRow(label: L10n.App.Packages.Filter.installed.localized, value: "\(core.installedPackages.count)")
                    StatRow(
                        label: L10n.App.Packages.Filter.upgradable.localized,
                        value: "\(core.outdatedPackages.count)",
                        valueColor: core.outdatedPackages.isEmpty ? .primary : .orange
                    )
                    StatRow(
                        label: L10n.App.Packages.Filter.available.localized,
                        value: "\(core.cachedAvailablePackages.count)",
                        valueColor: core.cachedAvailablePackages.isEmpty ? .secondary : .blue
                    )
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 10)

            Divider()
                .padding(.horizontal, 16)

            // Connection banner
            if !core.isConnected {
                HStack {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundColor(.yellow)
                    Text(L10n.App.Dashboard.Status.reconnecting.localized)
                        .font(.caption)
                    Spacer()
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 6)
                .background(Color.yellow.opacity(0.1))
                .cornerRadius(6)
                .padding(.horizontal, 12)
            }

            // Manager grid (installed + enabled only)
            VStack(alignment: .leading, spacing: 8) {
                Text(L10n.App.Dashboard.Section.managers.localized)
                    .font(.subheadline)
                    .fontWeight(.semibold)
                    .padding(.horizontal, 16)

                if activeManagers.isEmpty {
                    Text(L10n.App.Dashboard.State.emptyManagers.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .padding(.horizontal, 16)
                } else {
                    LazyVGrid(
                        columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 4),
                        spacing: 12
                    ) {
                        ForEach(activeManagers) { manager in
                            ManagerItemView(
                                manager: manager,
                                packageCount: countFor(manager: manager),
                                hasOutdatedPackages: hasOutdated(manager: manager),
                                hasFailedTasks: hasFailed(manager: manager),
                                versionAvailable: {
                                    if let version = core.managerStatuses[manager.id]?.version {
                                        return !version.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                    }
                                    return false
                                }(),
                                outdatedCount: outdatedCount(manager: manager),
                                onTap: {
                                    core.selectedManagerFilter = normalizedManagerName(manager.id)
                                    selectedTab = .packages
                                },
                                onRefresh: {
                                    core.triggerRefresh()
                                }
                            )
                        }
                    }
                    .padding(.horizontal, 12)
                }
            }

            Divider()
                .padding(.horizontal, 16)

            // Recent tasks
            VStack(alignment: .leading, spacing: 6) {
                Text(L10n.App.Dashboard.Section.recentTasks.localized)
                    .font(.subheadline)
                    .fontWeight(.semibold)
                    .padding(.horizontal, 16)

                if core.activeTasks.isEmpty {
                    Text(L10n.App.Dashboard.State.emptyTasks.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .padding(.horizontal, 16)
                } else {
                    ScrollView {
                        VStack(spacing: 0) {
                            ForEach(core.activeTasks.prefix(20)) { task in
                                TaskRowView(task: task)
                                    .padding(.horizontal, 8)
                            }
                        }
                    }
                }
            }
            .frame(maxHeight: .infinity)
        }
        .padding(.bottom, 12)
    }

    private var activeManagers: [ManagerInfo] {
        ManagerInfo.all.filter { manager in
            let status = core.managerStatuses[manager.id]
            let detected = status?.detected ?? false
            let enabled = status?.enabled ?? true
            return manager.isImplemented && detected && enabled
        }
    }

    private func countFor(manager: ManagerInfo) -> Int {
        core.installedPackages.filter {
            $0.manager.lowercased().contains(manager.shortName.lowercased())
        }.count
    }

    private func hasOutdated(manager: ManagerInfo) -> Bool {
        core.outdatedPackages.contains {
            $0.manager.lowercased().contains(manager.shortName.lowercased())
        }
    }

    private func outdatedCount(manager: ManagerInfo) -> Int {
        core.outdatedPackages.filter {
            $0.manager.lowercased().contains(manager.shortName.lowercased())
        }.count
    }

    private func hasFailed(manager: ManagerInfo) -> Bool {
        core.activeTasks.contains {
            $0.status.lowercased() == "failed" &&
            $0.description.lowercased().contains(manager.shortName.lowercased())
        }
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

private struct StatRow: View {
    let label: String
    let value: String
    var valueColor: Color = .primary

    var body: some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.caption)
                .foregroundColor(.secondary)
            Text(value)
                .font(.caption)
                .fontWeight(.medium)
                .foregroundColor(valueColor)
        }
    }
}
