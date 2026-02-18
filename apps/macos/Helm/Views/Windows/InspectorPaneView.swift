import SwiftUI

struct InspectorPaneView: View {
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("inspector.title")
                .font(.headline)

            if let package = store.selectedPackage {
                packageDetail(package)
            } else if let manager = store.selectedManager {
                managerDetail(manager)
            } else {
                Text("inspector.empty")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            Spacer()
        }
        .padding(14)
        .frame(minWidth: 260, idealWidth: 280)
    }

    @ViewBuilder
    private func packageDetail(_ package: PackageRecord) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(package.name)
                .font(.title3.weight(.semibold))
            Text(package.managerDisplayName)
                .font(.callout)
                .foregroundStyle(.secondary)
            Text("inspector.installedVersion")
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(package.installedVersion)
                .font(.caption.monospaced())
            Text("inspector.latestVersion")
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(package.latestVersion)
                .font(.caption.monospaced())
            Text("inspector.cacheSource")
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(package.sourceQuery)
                .font(.caption)
        }
    }

    @ViewBuilder
    private func managerDetail(_ manager: ManagerHealth) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(manager.displayName)
                .font(.title3.weight(.semibold))
            Text(LocalizedStringKey(manager.authority.localizationKey))
                .font(.callout)
                .foregroundStyle(.secondary)
            Text("inspector.capabilities")
                .font(.caption)
                .foregroundStyle(.secondary)
            ForEach(manager.capabilitySummary, id: \.self) { capability in
                Text(LocalizedStringKey(capability))
                    .font(.caption)
            }
        }
    }
}
