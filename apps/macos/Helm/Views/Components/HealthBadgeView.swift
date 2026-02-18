import SwiftUI

struct HealthBadgeView: View {
    let status: HelmAggregateStatus

    var body: some View {
        Label {
            Text(LocalizedStringKey(status.localizationKey))
                .font(.caption.weight(.semibold))
        } icon: {
            Image(systemName: status.symbolName)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(status.color.opacity(0.14), in: Capsule())
        .foregroundStyle(status.color)
        .accessibilityLabel(Text(LocalizedStringKey(status.localizationKey)))
    }
}

private extension HelmAggregateStatus {
    var color: Color {
        switch self {
        case .healthy:
            return .green
        case .attention:
            return .orange
        case .error:
            return .red
        case .running:
            return .blue
        }
    }
}
