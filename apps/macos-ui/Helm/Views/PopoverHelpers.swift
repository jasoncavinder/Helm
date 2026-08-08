import SwiftUI

struct PopoverOverlayCard<Content: View>: View {
    let title: String
    let onClose: () -> Void
    var showsCloseButton: Bool = true
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(title)
                    .font(.headline)
                Spacer()
                if showsCloseButton {
                    Button(action: onClose) {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                    .keyboardShortcut(.escape, modifiers: [])
                    .helmPointer()
                    .accessibilityLabel(L10n.Common.close.localized)
                }
            }

            content
        }
        .padding(16)
        .frame(width: 360)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(HelmTheme.surfacePanel)
                .overlay(
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .strokeBorder(HelmTheme.borderSubtle.opacity(0.95), lineWidth: 0.8)
                )
                .shadow(color: Color.black.opacity(0.16), radius: 12, x: 0, y: 8)
        )
    }
}

struct PopoverAttentionBanner: View {
    @ObservedObject private var overviewState = HelmCore.shared.overviewState
    @EnvironmentObject private var context: ControlCenterContext
    @Environment(\.colorScheme) private var colorScheme
    let onOpenControlCenter: () -> Void

    private var projection: WayfinderProjectionContent {
        overviewState.wayfinderProjection.content
    }

    private var bannerSymbol: String {
        switch projection.mode {
        case .healthy:
            return "checkmark.circle.fill"
        case .updatesReady:
            return "arrow.up.circle.fill"
        case .determinateWork, .indeterminateWork:
            return "arrow.triangle.2.circlepath"
        case .approval:
            return "hand.raised.circle.fill"
        case .failedInterrupted:
            return "exclamationmark.octagon.fill"
        case .cachedPartialOffline:
            return "bolt.horizontal.circle.fill"
        }
    }

    private var bannerTint: Color {
        switch projection.mode {
        case .failedInterrupted:
            return colorScheme == .dark
                ? Color(red: 1.0, green: 120.0 / 255.0, blue: 120.0 / 255.0)
                : Color(red: 224.0 / 255.0, green: 58.0 / 255.0, blue: 58.0 / 255.0)
        case .determinateWork, .indeterminateWork:
            return HelmTheme.blue500
        case .healthy:
            return HelmTheme.stateHealthy
        case .updatesReady, .approval, .cachedPartialOffline:
            return colorScheme == .dark
                ? Color(red: 244.0 / 255.0, green: 203.0 / 255.0, blue: 92.0 / 255.0)
                : Color(red: 204.0 / 255.0, green: 152.0 / 255.0, blue: 36.0 / 255.0)
        }
    }

    private var bannerBackgroundOpacity: Double {
        projection.mode == .failedInterrupted ? 0.16 : 0.14
    }

    private var bannerBorderOpacity: Double {
        projection.mode == .failedInterrupted ? 0.38 : 0.32
    }

    private var bannerTitle: String {
        projection.title.localized
    }

    private var bannerMessage: String {
        projection.explanation.localized
    }

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: bannerSymbol)
                .foregroundColor(bannerTint)
                .font(.system(size: 13, weight: .semibold))

            VStack(alignment: .leading, spacing: 2) {
                Text(bannerTitle)
                    .font(.caption.weight(.semibold))
                Text(bannerMessage)
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }

            Spacer(minLength: 10)

            Button(projection.primaryActionTitle.localized) {
                context.navigate(to: projection.primaryAction)
                onOpenControlCenter()
            }
            .buttonStyle(UpdateAllPillButtonStyle())
            .helmPointer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(bannerTint.opacity(bannerBackgroundOpacity))
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(bannerTint.opacity(bannerBorderOpacity), lineWidth: 0.8)
                )
        )
    }

}

struct PopoverSearchField: View {
    @ObservedObject private var core = HelmCore.shared
    @Binding var popoverSearchQuery: String
    let onSyncSearchQuery: (String) -> Void
    let onActivateSearch: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundColor(.secondary)
            TextField(
                "app.popover.search_placeholder".localized,
                text: Binding(
                    get: { popoverSearchQuery },
                    set: { newValue in
                        popoverSearchQuery = newValue
                        onSyncSearchQuery(newValue)
                    }
                )
            )
            .textFieldStyle(.plain)
            .font(.subheadline)

            if core.isSearching {
                ProgressView()
                    .controlSize(.small)
            }

            if !popoverSearchQuery.isEmpty {
                Button {
                    popoverSearchQuery = ""
                    onSyncSearchQuery("")
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
                .helmPointer()
                .accessibilityLabel(L10n.Common.clear.localized)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(HelmTheme.surfacePanel)
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(HelmTheme.borderSubtle.opacity(0.95), lineWidth: 0.8)
                )
        )
        .onTapGesture {
            onActivateSearch()
        }
        .helmPointer()
        .accessibilityLabel(L10n.App.Popover.searchPlaceholder.localized)
    }
}

struct UpdateAllPillButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.caption.weight(.bold))
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .foregroundColor(Color.white)
            .background(
                Capsule(style: .continuous)
                    .fill(configuration.isPressed ? HelmTheme.actionPrimaryPressed : HelmTheme.actionPrimaryDefault)
                    .overlay(
                        Capsule(style: .continuous)
                            .strokeBorder(Color.white.opacity(0.16), lineWidth: 0.8)
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.97 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.1),
                value: configuration.isPressed
            )
    }
}

struct MetricChipView: View {
    let label: String
    let value: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label)
                .font(.caption2)
                .foregroundColor(.secondary)
            Text("\(value)")
                .font(.callout.monospacedDigit().weight(.semibold))
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(HelmTheme.surfaceElevated)
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .strokeBorder(HelmTheme.borderSubtle.opacity(0.9), lineWidth: 0.8)
                )
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel(label)
        .accessibilityValue("\(value)")
    }
}
