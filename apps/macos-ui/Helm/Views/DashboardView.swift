import SwiftUI

struct DashboardView: View {
    @ObservedObject var core = HelmCore.shared

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                // App icon + version
                VStack(spacing: 6) {
                    Image(nsImage: NSApp.applicationIconImage)
                        .resizable()
                        .frame(width: 64, height: 64)
                        .cornerRadius(14)

                    Text("Helm")
                        .font(.title2)
                        .fontWeight(.bold)

                    Text("v\(helmVersion)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                .padding(.top, 8)

                // Stats
                VStack(spacing: 6) {
                    LabeledContentRow(
                        label: "Installed",
                        value: "\(core.installedPackages.count)"
                    )
                    LabeledContentRow(
                        label: "Upgradable",
                        value: "\(core.outdatedPackages.count)",
                        valueColor: core.outdatedPackages.isEmpty ? .primary : .orange
                    )
                    LabeledContentRow(
                        label: "Available",
                        value: "--",
                        valueColor: .secondary
                    )
                }
                .padding(.horizontal, 16)

                Divider()
                    .padding(.horizontal, 16)

                // Connection banner
                if !core.isConnected {
                    HStack {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundColor(.yellow)
                        Text("Reconnecting to service...")
                            .font(.caption)
                        Spacer()
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 6)
                    .background(Color.yellow.opacity(0.1))
                    .cornerRadius(6)
                    .padding(.horizontal, 12)
                }

                // Manager grid
                VStack(alignment: .leading, spacing: 8) {
                    Text("Package Managers")
                        .font(.subheadline)
                        .fontWeight(.semibold)
                        .padding(.horizontal, 16)

                    LazyVGrid(
                        columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 4),
                        spacing: 12
                    ) {
                        ForEach(ManagerInfo.all) { manager in
                            ManagerItemView(
                                manager: manager,
                                packageCount: countFor(manager: manager)
                            )
                        }
                    }
                    .padding(.horizontal, 12)
                }

                Divider()
                    .padding(.horizontal, 16)

                // Recent tasks
                VStack(alignment: .leading, spacing: 6) {
                    Text("Recent Tasks")
                        .font(.subheadline)
                        .fontWeight(.semibold)
                        .padding(.horizontal, 16)

                    if core.activeTasks.isEmpty {
                        Text("No recent tasks")
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .padding(.horizontal, 16)
                    } else {
                        ForEach(core.activeTasks.prefix(5)) { task in
                            TaskRowView(task: task)
                                .padding(.horizontal, 8)
                        }
                    }
                }
            }
            .padding(.bottom, 12)
        }
    }

    private func countFor(manager: ManagerInfo) -> Int {
        core.installedPackages.filter {
            $0.manager.lowercased().contains(manager.shortName.lowercased())
        }.count
    }
}
