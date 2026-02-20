import SwiftUI

struct FilterButton: View {
    let title: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.caption)
                .fontWeight(isSelected ? .semibold : .regular)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(isSelected ? HelmTheme.selectionFill : HelmTheme.surfaceElevated)
                        .overlay(
                            RoundedRectangle(cornerRadius: 6, style: .continuous)
                                .strokeBorder(
                                    isSelected ? HelmTheme.selectionStroke : HelmTheme.borderSubtle.opacity(0.85),
                                    lineWidth: 0.8
                                )
                        )
                )
                .foregroundColor(isSelected ? HelmTheme.actionPrimaryDefault : HelmTheme.textSecondary)
        }
        .buttonStyle(.plain)
        .helmPointer()
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityLabel(title)
    }
}
