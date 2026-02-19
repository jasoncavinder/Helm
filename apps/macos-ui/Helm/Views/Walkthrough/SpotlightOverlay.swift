import SwiftUI

// MARK: - Anchor Preference Key

struct SpotlightAnchorKey: PreferenceKey {
    static var defaultValue: [String: Anchor<CGRect>] = [:]

    static func reduce(
        value: inout [String: Anchor<CGRect>],
        nextValue: () -> [String: Anchor<CGRect>]
    ) {
        value.merge(nextValue()) { _, new in new }
    }
}

// MARK: - View Extension

extension View {
    func spotlightAnchor(_ name: String) -> some View {
        anchorPreference(key: SpotlightAnchorKey.self, value: .bounds) { anchor in
            [name: anchor]
        }
    }
}

// MARK: - Cutout Shape

struct SpotlightCutoutShape: Shape {
    var cutoutRect: CGRect
    var cornerRadius: CGFloat = 8

    var animatableData: AnimatablePair<CGFloat, AnimatablePair<CGFloat, AnimatablePair<CGFloat, CGFloat>>> {
        get {
            AnimatablePair(
                cutoutRect.origin.x,
                AnimatablePair(
                    cutoutRect.origin.y,
                    AnimatablePair(cutoutRect.size.width, cutoutRect.size.height)
                )
            )
        }
        set {
            cutoutRect = CGRect(
                x: newValue.first,
                y: newValue.second.first,
                width: newValue.second.second.first,
                height: newValue.second.second.second
            )
        }
    }

    func path(in rect: CGRect) -> Path {
        let paddedCutout = cutoutRect.insetBy(dx: -6, dy: -4)
        return Path(
            roundedRect: paddedCutout,
            cornerSize: CGSize(width: cornerRadius, height: cornerRadius),
            style: .continuous
        )
    }
}

// MARK: - Tooltip Card

struct WalkthroughTooltipCard: View {
    let step: WalkthroughStepDefinition
    let currentIndex: Int
    let totalSteps: Int
    let onNext: () -> Void
    let onSkip: () -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var isLastStep: Bool {
        currentIndex + 1 >= totalSteps
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(step.titleKey.localized)
                .font(.headline)
                .foregroundStyle(Color.primary)

            Text(step.descriptionKey.localized)
                .font(.subheadline)
                .foregroundStyle(Color.secondary)
                .fixedSize(horizontal: false, vertical: true)

            HStack {
                stepDots

                Spacer()

                Button(action: onSkip) {
                    Text(L10n.App.Walkthrough.Control.skip.localized)
                        .font(.caption)
                        .foregroundStyle(Color.secondary)
                }
                .buttonStyle(.plain)
                .helmPointer()

                Button(action: onNext) {
                    Text(
                        isLastStep
                            ? L10n.App.Walkthrough.Control.done.localized
                            : L10n.App.Walkthrough.Control.next.localized
                    )
                }
                .buttonStyle(HelmPrimaryButtonStyle(
                    cornerRadius: 8,
                    horizontalPadding: 14,
                    verticalPadding: 5
                ))
                .helmPointer()
            }
        }
        .padding(14)
        .frame(width: 280)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(.ultraThickMaterial)
                .shadow(color: .black.opacity(0.2), radius: 12, y: 4)
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel(step.titleKey.localized)
        .accessibilityValue(step.descriptionKey.localized)
        .accessibilityAddTraits(.isStaticText)
        .accessibilityAction(named: L10n.App.Walkthrough.Control.skip.localized) {
            onSkip()
        }
    }

    private var stepDots: some View {
        HStack(spacing: 4) {
            ForEach(0..<totalSteps, id: \.self) { index in
                Circle()
                    .fill(index == currentIndex ? Color.orange : Color.primary.opacity(0.2))
                    .frame(width: 6, height: 6)
            }
        }
    }
}

// MARK: - Spotlight Overlay

struct SpotlightOverlay: View {
    @ObservedObject var manager: WalkthroughManager
    let anchors: [String: Anchor<CGRect>]

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        if let step = manager.currentStep {
            GeometryReader { proxy in
                let targetRect = resolveTargetRect(step: step, proxy: proxy)
                ZStack {
                    Color.black.opacity(0.5)
                        .mask(
                            ZStack {
                                Color.white
                                SpotlightCutoutShape(cutoutRect: targetRect)
                                    .fill(Color.white)
                                    .blur(radius: 3)
                                    .blendMode(.destinationOut)
                            }
                            .compositingGroup()
                        )
                        .allowsHitTesting(true)
                        .onTapGesture {
                            manager.advanceStep()
                        }

                    tooltipPositioned(step: step, targetRect: targetRect, proxy: proxy)
                }
                .animation(
                    reduceMotion ? .easeOut(duration: 0.14) : .spring(response: 0.3, dampingFraction: 0.85),
                    value: manager.currentStepIndex
                )
            }
            .ignoresSafeArea()
            .transition(reduceMotion ? .opacity : .opacity.combined(with: .scale(scale: 0.98)))
        }
    }

    private func resolveTargetRect(step: WalkthroughStepDefinition, proxy: GeometryProxy) -> CGRect {
        if let anchor = anchors[step.targetAnchor] {
            return proxy[anchor]
        }
        return CGRect(x: proxy.size.width / 2 - 50, y: proxy.size.height / 2 - 20, width: 100, height: 40)
    }

    private func tooltipPositioned(
        step: WalkthroughStepDefinition,
        targetRect: CGRect,
        proxy: GeometryProxy
    ) -> some View {
        let tooltipSize = CGSize(width: 280, height: 140)
        let position = calculateTooltipPosition(
            edge: step.tooltipEdge,
            targetRect: targetRect,
            tooltipSize: tooltipSize,
            containerSize: proxy.size
        )

        return WalkthroughTooltipCard(
            step: step,
            currentIndex: manager.currentStepIndex,
            totalSteps: manager.totalSteps,
            onNext: { manager.advanceStep() },
            onSkip: { manager.skip() }
        )
        .position(position)
    }

    private func calculateTooltipPosition(
        edge: Edge,
        targetRect: CGRect,
        tooltipSize: CGSize,
        containerSize: CGSize
    ) -> CGPoint {
        let padding: CGFloat = 12
        var point: CGPoint

        switch edge {
        case .top:
            point = CGPoint(
                x: targetRect.midX,
                y: targetRect.minY - padding - tooltipSize.height / 2
            )
        case .bottom:
            point = CGPoint(
                x: targetRect.midX,
                y: targetRect.maxY + padding + tooltipSize.height / 2
            )
        case .leading:
            point = CGPoint(
                x: targetRect.minX - padding - tooltipSize.width / 2,
                y: targetRect.midY
            )
        case .trailing:
            point = CGPoint(
                x: targetRect.maxX + padding + tooltipSize.width / 2,
                y: targetRect.midY
            )
        }

        let halfWidth = tooltipSize.width / 2
        let halfHeight = tooltipSize.height / 2
        point.x = max(halfWidth + 8, min(containerSize.width - halfWidth - 8, point.x))
        point.y = max(halfHeight + 8, min(containerSize.height - halfHeight - 8, point.y))

        return point
    }
}
