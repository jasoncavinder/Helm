import SwiftUI
import AppKit

struct HelmPrimaryButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    var cornerRadius: CGFloat = 10
    var horizontalPadding: CGFloat = 12
    var verticalPadding: CGFloat = 7

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(Color.white)
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(
                        LinearGradient(
                            colors: [Color.orange, Color.red.opacity(0.86)],
                            startPoint: .leading,
                            endPoint: .trailing
                        )
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.98 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.12),
                value: configuration.isPressed
            )
    }
}

struct HelmSecondaryButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    var cornerRadius: CGFloat = 10
    var horizontalPadding: CGFloat = 12
    var verticalPadding: CGFloat = 7

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(Color.primary)
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(Color.primary.opacity(configuration.isPressed ? 0.14 : 0.09))
                    .overlay(
                        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                            .strokeBorder(Color.primary.opacity(0.12), lineWidth: 0.8)
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.985 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.12),
                value: configuration.isPressed
            )
    }
}

struct HelmIconButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(Color.primary)
            .frame(width: 28, height: 28)
            .background(
                Circle()
                    .fill(Color.primary.opacity(configuration.isPressed ? 0.14 : 0.09))
                    .overlay(
                        Circle()
                            .strokeBorder(Color.primary.opacity(0.12), lineWidth: 0.8)
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.97 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.1),
                value: configuration.isPressed
            )
    }
}

private struct HelmPointerModifier: ViewModifier {
    let enabled: Bool

    func body(content: Content) -> some View {
        content
            .onHover { hovering in
                guard enabled else {
                    NSCursor.arrow.set()
                    return
                }
                if hovering {
                    NSCursor.pointingHand.set()
                } else {
                    NSCursor.arrow.set()
                }
            }
            .onDisappear {
                NSCursor.arrow.set()
            }
    }
}

extension View {
    func helmPointer(enabled: Bool = true) -> some View {
        modifier(HelmPointerModifier(enabled: enabled))
    }
}
