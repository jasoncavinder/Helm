import Foundation
import SwiftUI

struct WayfinderDashboardHero: View {
    let projection: WayfinderProjectionContent
    let isRefreshing: Bool
    let onPrimaryAction: () -> Void
    let onRefresh: () -> Void

    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.colorSchemeContrast) private var contrast

    private var showsProjectionAction: Bool {
        projection.condition != .healthy && projection.condition != .refreshing
    }

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 28) {
                courseIndicator
                heroCopy
                Spacer(minLength: 0)
            }

            VStack(alignment: .leading, spacing: 20) {
                courseIndicator
                heroCopy
            }
        }
        .padding(.horizontal, 28)
        .padding(.vertical, 24)
        .frame(maxWidth: .infinity, minHeight: 224, alignment: .leading)
        .background(heroBackground)
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .strokeBorder(
                    HelmTheme.blue500.opacity(contrast == .increased ? 0.5 : 0.2),
                    lineWidth: contrast == .increased ? 1.5 : 1
                )
        }
    }

    private var courseIndicator: some View {
        WayfinderCourseIndicatorView(projection: projection)
            .frame(width: 168, height: 168)
    }

    private var heroCopy: some View {
        VStack(alignment: .leading, spacing: 9) {
            Text("app.wayfinder.hero.eyebrow".localized)
                .font(.caption2.weight(.bold))
                .tracking(1.2)
                .foregroundColor(HelmTheme.blue500)

            Text(projection.title.localized)
                .font(.system(.largeTitle, design: .rounded, weight: .semibold))
                .foregroundColor(HelmTheme.textPrimary)
                .fixedSize(horizontal: false, vertical: true)

            Text(projection.explanation.localized)
                .font(.body)
                .foregroundColor(HelmTheme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)

            HStack(spacing: 10) {
                if showsProjectionAction {
                    Button(projection.primaryActionTitle.localized, action: onPrimaryAction)
                        .buttonStyle(HelmPrimaryButtonStyle())
                }

                if showsProjectionAction {
                    Button(L10n.App.Settings.Action.refreshNow.localized, action: onRefresh)
                        .buttonStyle(HelmSecondaryButtonStyle())
                        .disabled(isRefreshing)
                } else {
                    Button(L10n.App.Settings.Action.refreshNow.localized, action: onRefresh)
                        .buttonStyle(HelmPrimaryButtonStyle())
                        .disabled(isRefreshing)
                }
            }
            .padding(.top, 4)
        }
        .frame(maxWidth: 560, alignment: .leading)
    }

    private var heroBackground: some View {
        ZStack {
            if reduceTransparency {
                HelmTheme.surfacePanel
            } else {
                LinearGradient(
                    colors: colorScheme == .dark
                        ? [
                            HelmTheme.blue900.opacity(0.5),
                            HelmTheme.seaGlass.opacity(0.14),
                            HelmTheme.surfacePanel.opacity(0.92)
                        ]
                        : [
                            HelmTheme.blue500.opacity(0.14),
                            HelmTheme.seaGlass.opacity(0.09),
                            HelmTheme.surfacePanel.opacity(0.96)
                        ],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            }

            WayfinderHorizonLines()
                .stroke(
                    HelmTheme.horizon.opacity(
                        reduceTransparency ? 0.08 : (colorScheme == .dark ? 0.14 : 0.1)
                    ),
                    lineWidth: 1
                )
        }
    }
}

struct WayfinderCourseIndicatorView: View {
    let projection: WayfinderProjectionContent

    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    private var style: WayfinderCourseStyle {
        WayfinderCourseStyle(mode: projection.mode)
    }

    private var accessibilityValue: String {
        guard let progress = projection.progress else {
            return projection.explanation.localized
        }
        let percent = NumberFormatter.localizedString(
            from: NSNumber(value: progress.fraction),
            number: .percent
        )
        return "\(percent). \(projection.explanation.localized)"
    }

    var body: some View {
        TimelineView(
            .animation(
                minimumInterval: 1.0 / 30.0,
                paused: reduceMotion || !style.rotates
            )
        ) { timeline in
            ZStack {
                Circle()
                    .stroke(
                        style.tint.opacity(contrast == .increased ? 0.32 : 0.16),
                        lineWidth: contrast == .increased ? 15 : 13
                    )

                courseArc(at: timeline.date)

                Circle()
                    .fill(
                        reduceTransparency
                            ? HelmTheme.surfacePanel
                            : HelmTheme.surfaceElevated.opacity(0.94)
                    )
                    .padding(24)

                tickMarks
                centerContent
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityIdentifier("wayfinderCourseIndicator")
        .accessibilityLabel(projection.title.localized)
        .accessibilityValue(accessibilityValue)
    }

    private func courseArc(at date: Date) -> some View {
        let progress = projection.progress?.fraction
        let arcEnd = progress ?? style.arcEnd
        let rotation = style.rotates && !reduceMotion
            ? date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.8) / 1.8 * 360
            : 0

        return Circle()
            .trim(from: progress == nil ? style.arcStart : 0, to: arcEnd)
            .stroke(
                AngularGradient(
                    colors: [style.tint.opacity(0.74), style.tint, HelmTheme.horizon],
                    center: .center
                ),
                style: StrokeStyle(
                    lineWidth: contrast == .increased ? 15 : 13,
                    lineCap: .round,
                    dash: differentiateWithoutColor ? [5, 4] : style.dash
                )
            )
            .rotationEffect(.degrees(-90 + rotation))
    }

    private var tickMarks: some View {
        ForEach(0..<8, id: \.self) { index in
            Capsule()
                .fill(HelmTheme.textSecondary.opacity(contrast == .increased ? 0.62 : 0.34))
                .frame(width: 2, height: 8)
                .offset(y: -56)
                .rotationEffect(.degrees(Double(index) * 45))
        }
    }

    private var centerContent: some View {
        VStack(spacing: 6) {
            Image(systemName: style.symbol)
                .font(.system(size: 31, weight: .medium))
                .foregroundColor(style.tint)

            if let progress = projection.progress {
                Text(progress.fraction, format: .percent.precision(.fractionLength(0)))
                    .font(.caption.weight(.bold).monospacedDigit())
                    .foregroundColor(HelmTheme.textPrimary)
            } else {
                Text(projection.title.localized)
                    .font(.caption2.weight(.bold))
                    .foregroundColor(HelmTheme.textSecondary)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                    .minimumScaleFactor(0.72)
                    .padding(.horizontal, 34)
            }
        }
    }
}

private struct WayfinderCourseStyle {
    let tint: Color
    let symbol: String
    let arcStart: CGFloat
    let arcEnd: CGFloat
    let dash: [CGFloat]
    let rotates: Bool

    init(mode: WayfinderCourseMode) {
        switch mode {
        case .healthy:
            tint = HelmTheme.stateHealthy
            symbol = "checkmark"
            arcStart = 0
            arcEnd = 1
            dash = []
            rotates = false
        case .updatesReady:
            tint = HelmTheme.stateAttention
            symbol = "arrow.up"
            arcStart = 0.03
            arcEnd = 0.76
            dash = []
            rotates = false
        case .determinateWork:
            tint = HelmTheme.stateRunning
            symbol = "arrow.triangle.2.circlepath"
            arcStart = 0
            arcEnd = 1
            dash = []
            rotates = false
        case .indeterminateWork:
            tint = HelmTheme.stateRunning
            symbol = "arrow.triangle.2.circlepath"
            arcStart = 0.03
            arcEnd = 0.7
            dash = []
            rotates = true
        case .approval:
            tint = HelmTheme.stateAttention
            symbol = "hand.raised.fill"
            arcStart = 0.08
            arcEnd = 0.68
            dash = [7, 5]
            rotates = false
        case .failedInterrupted:
            tint = HelmTheme.stateError
            symbol = "exclamationmark"
            arcStart = 0.02
            arcEnd = 0.9
            dash = [4, 3]
            rotates = false
        case .cachedPartialOffline:
            tint = HelmTheme.stateAttention
            symbol = "bolt.horizontal.fill"
            arcStart = 0.1
            arcEnd = 0.62
            dash = [3, 5]
            rotates = false
        }
    }
}

private struct WayfinderHorizonLines: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        let spacing: CGFloat = 22
        var y: CGFloat = -rect.height

        while y < rect.height * 2 {
            path.move(to: CGPoint(x: 0, y: y))
            path.addLine(to: CGPoint(x: rect.width, y: y + rect.width * 0.28))
            y += spacing
        }
        return path
    }
}
