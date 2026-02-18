import SwiftUI

struct ManagerHealthCardView: View {
    let manager: ManagerHealth

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(manager.displayName)
                    .font(.headline)
                Spacer()
                HealthBadgeView(status: manager.status)
            }

            HStack(spacing: 8) {
                Text("manager.authority")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(LocalizedStringKey(manager.authority.localizationKey))
                    .font(.caption.weight(.medium))
            }

            HStack(spacing: 6) {
                Text("manager.outdated")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("\(manager.outdatedCount)")
                    .font(.caption.monospacedDigit())
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}
