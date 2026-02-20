import SwiftUI
import AppKit

enum HelmTheme {
    static let blue900 = Color.helmDynamic(light: 0x1B3A66, dark: 0x2A5DA8)
    static let blue700 = Color.helmDynamic(light: 0x2A5DA8, dark: 0x3C7DD9)
    static let blue500 = Color.helmDynamic(light: 0x3C7DD9, dark: 0x6CA6E8)

    static let proAccent = Color.helmDynamic(light: 0xC89C3D, dark: 0xC89C3D)
    static let proAccentTop = Color.helmDynamic(light: 0xD7AF5A, dark: 0xD0A64E)
    static let proAccentDeep = Color.helmDynamic(light: 0xA97E2A, dark: 0xA97E2A)
    static let proOnAccent = Color.helmDynamic(light: 0x1C1F26, dark: 0x1C1F26)

    static let surfaceBase = Color.helmDynamic(light: 0xF5F7FA, dark: 0x0E1624)
    static let surfacePanel = Color.helmDynamic(light: 0xFFFFFF, dark: 0x141E2F)
    static let surfaceElevated = Color.helmDynamic(light: 0xFBFCFE, dark: 0x18263A)
    static let borderSubtle = Color.helmDynamic(light: 0xE2E6EC, dark: 0x24324A)
    static let statusRail = Color.helmDynamic(light: 0xD9E2F0, dark: 0x2A3C57)

    static let textPrimary = Color.helmDynamic(light: 0x1C1F26, dark: 0xE6EDF6)
    static let textSecondary = Color.helmDynamic(light: 0x4B5563, dark: 0x9FB0C7)

    static let actionPrimaryDefault = Color.helmDynamic(light: 0x2A5DA8, dark: 0x2F69BB)
    static let actionPrimaryHover = Color.helmDynamic(light: 0x3676CE, dark: 0x3676CE)
    static let actionPrimaryPressed = Color.helmDynamic(light: 0x1B3A66, dark: 0x2A5DA8)
    static let actionSecondaryText = Color.helmDynamic(light: 0x2A5DA8, dark: 0x6CA6E8)
    static let actionSecondaryBorder = Color.helmDynamic(light: 0x3C7DD9, dark: 0x6CA6E8)

    static let stateHealthy = Color.helmDynamic(light: 0x2F855A, dark: 0x4FB382)
    static let stateAttention = Color.helmDynamic(light: 0x8C691F, dark: 0xC89C3D)
    static let stateError = Color.helmDynamic(light: 0xD64545, dark: 0xF06A6A)
    static let stateRunning = blue500

    static let selectionFill = Color.helmDynamic(
        light: 0x3C7DD9,
        dark: 0x6CA6E8,
        lightAlpha: 0.12,
        darkAlpha: 0.16
    )
    static let selectionStroke = Color.helmDynamic(
        light: 0x3C7DD9,
        dark: 0x6CA6E8,
        lightAlpha: 0.4,
        darkAlpha: 0.52
    )
}

enum HelmMetrics {
    static let radiusControl: CGFloat = 12
    static let radiusCard: CGFloat = 16
    static let radiusChip: CGFloat = 8
}

private struct HelmCardSurfaceModifier: ViewModifier {
    @Environment(\.colorScheme) private var colorScheme
    let cornerRadius: CGFloat
    let highlighted: Bool
    let pro: Bool

    func body(content: Content) -> some View {
        content
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(highlighted ? HelmTheme.surfaceElevated : HelmTheme.surfacePanel)
                    .overlay(
                        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                            .strokeBorder(
                                pro
                                    ? HelmTheme.proAccent.opacity(colorScheme == .dark ? 0.52 : 0.5)
                                    : (
                                        highlighted
                                            ? HelmTheme.actionSecondaryBorder.opacity(colorScheme == .dark ? 0.45 : 0.38)
                                            : HelmTheme.borderSubtle.opacity(colorScheme == .dark ? 0.88 : 0.95)
                                    ),
                                lineWidth: 0.9
                            )
                    )
                    .shadow(
                        color: Color.black.opacity(
                            colorScheme == .dark
                                ? (pro ? 0.26 : 0.18)
                                : (pro ? 0.12 : 0.07)
                        ),
                        radius: pro ? 14 : (highlighted ? 10 : 8),
                        x: 0,
                        y: pro ? 6 : 4
                    )
            )
    }
}

struct HelmPrimaryButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    @Environment(\.isEnabled) private var isEnabled
    var cornerRadius: CGFloat = HelmMetrics.radiusControl
    var horizontalPadding: CGFloat = 12
    var verticalPadding: CGFloat = 8

    func makeBody(configuration: Configuration) -> some View {
        let fillColor: Color = {
            guard isEnabled else { return HelmTheme.actionPrimaryDefault.opacity(0.45) }
            if configuration.isPressed {
                return HelmTheme.actionPrimaryPressed
            }
            return HelmTheme.actionPrimaryDefault
        }()

        configuration.label
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(Color.white)
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(fillColor)
                    .overlay(
                        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                            .strokeBorder(Color.white.opacity(0.18), lineWidth: 0.8)
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.98 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.12),
                value: configuration.isPressed
            )
            .onHover { hovering in
                guard isEnabled, !accessibilityReduceMotion else { return }
                if hovering {
                    NSCursor.pointingHand.set()
                } else {
                    NSCursor.arrow.set()
                }
            }
    }
}

struct HelmSecondaryButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    @Environment(\.isEnabled) private var isEnabled
    var cornerRadius: CGFloat = HelmMetrics.radiusControl
    var horizontalPadding: CGFloat = 12
    var verticalPadding: CGFloat = 8

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(isEnabled ? HelmTheme.actionSecondaryText : HelmTheme.textSecondary.opacity(0.78))
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(HelmTheme.actionPrimaryDefault.opacity(configuration.isPressed ? 0.18 : 0.08))
                    .overlay(
                        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                            .strokeBorder(
                                HelmTheme.actionSecondaryBorder.opacity(configuration.isPressed ? 0.48 : 0.35),
                                lineWidth: 0.8
                            )
                    )
            )
            .opacity(isEnabled ? 1 : 0.6)
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.985 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.12),
                value: configuration.isPressed
            )
    }
}

struct HelmTertiaryButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(isEnabled ? HelmTheme.textSecondary : HelmTheme.textSecondary.opacity(0.72))
            .opacity(configuration.isPressed ? 0.8 : 1)
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.985 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.12),
                value: configuration.isPressed
            )
    }
}

struct HelmProButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.isEnabled) private var isEnabled
    var cornerRadius: CGFloat = HelmMetrics.radiusControl
    var horizontalPadding: CGFloat = 12
    var verticalPadding: CGFloat = 8

    func makeBody(configuration: Configuration) -> some View {
        let fillGradient: LinearGradient = {
            guard isEnabled else {
                return LinearGradient(
                    colors: [
                        HelmTheme.proAccentTop.opacity(0.42),
                        HelmTheme.proAccentDeep.opacity(0.38)
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                )
            }

            if configuration.isPressed {
                return LinearGradient(
                    colors: [
                        HelmTheme.proAccent.opacity(0.96),
                        HelmTheme.proAccentDeep
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                )
            }

            return LinearGradient(
                colors: [
                    HelmTheme.proAccentTop,
                    HelmTheme.proAccent
                ],
                startPoint: .top,
                endPoint: .bottom
            )
        }()

        return configuration.label
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(HelmTheme.proOnAccent)
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(fillGradient)
                    .overlay(
                        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                            .strokeBorder(
                                LinearGradient(
                                    colors: [
                                        Color.white.opacity(configuration.isPressed ? 0.18 : 0.3),
                                        Color.white.opacity(0.05)
                                    ],
                                    startPoint: .top,
                                    endPoint: .bottom
                                ),
                                lineWidth: 0.8
                            )
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                            .strokeBorder(HelmTheme.proAccentDeep.opacity(configuration.isPressed ? 0.65 : 0.56), lineWidth: 0.8)
                    )
                    .shadow(
                        color: Color.black.opacity(
                            colorScheme == .dark
                                ? (configuration.isPressed ? 0.2 : 0.28)
                                : (configuration.isPressed ? 0.08 : 0.14)
                        ),
                        radius: configuration.isPressed ? 3 : 6,
                        x: 0,
                        y: configuration.isPressed ? 1 : 3
                    )
                    .shadow(
                        color: HelmTheme.proAccent.opacity(
                            colorScheme == .dark
                                ? (configuration.isPressed ? 0.1 : 0.18)
                                : (configuration.isPressed ? 0.05 : 0.1)
                        ),
                        radius: configuration.isPressed ? 0 : 1.5,
                        x: 0,
                        y: 0
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.98 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.12),
                value: configuration.isPressed
            )
    }
}

struct HelmIconButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(isEnabled ? HelmTheme.actionPrimaryDefault : HelmTheme.textSecondary.opacity(0.78))
            .frame(width: 28, height: 28)
            .background(
                Circle()
                    .fill(HelmTheme.actionPrimaryDefault.opacity(configuration.isPressed ? 0.18 : 0.08))
                    .overlay(
                        Circle()
                            .strokeBorder(
                                HelmTheme.actionSecondaryBorder.opacity(configuration.isPressed ? 0.48 : 0.34),
                                lineWidth: 0.8
                            )
                    )
            )
            .opacity(isEnabled ? 1 : 0.6)
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

    func helmCardSurface(
        cornerRadius: CGFloat = HelmMetrics.radiusCard,
        highlighted: Bool = false,
        pro: Bool = false
    ) -> some View {
        modifier(
            HelmCardSurfaceModifier(
                cornerRadius: cornerRadius,
                highlighted: highlighted,
                pro: pro
            )
        )
    }
}

private extension Color {
    static func helmDynamic(
        light: UInt32,
        dark: UInt32,
        lightAlpha: CGFloat = 1,
        darkAlpha: CGFloat = 1
    ) -> Color {
        Color(
            nsColor: NSColor(name: nil) { appearance in
                let resolvedAppearance = appearance.bestMatch(from: [.darkAqua, .aqua]) ?? .aqua
                if resolvedAppearance == .darkAqua {
                    return NSColor.helmHex(dark, alpha: darkAlpha)
                }
                return NSColor.helmHex(light, alpha: lightAlpha)
            }
        )
    }
}

private extension NSColor {
    static func helmHex(_ hex: UInt32, alpha: CGFloat = 1) -> NSColor {
        let red = CGFloat((hex >> 16) & 0xFF) / 255
        let green = CGFloat((hex >> 8) & 0xFF) / 255
        let blue = CGFloat(hex & 0xFF) / 255
        return NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
    }
}
