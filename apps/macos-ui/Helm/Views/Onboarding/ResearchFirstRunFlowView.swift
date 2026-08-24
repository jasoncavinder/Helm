import AppKit
import SwiftUI

struct ResearchFirstRunFlowView: View {
    let projection: ResearchFirstRunProjection
    let onComplete: () -> Void

    @State private var session = ResearchFirstRunSession()
    @State private var summaryCopied = false
    @AccessibilityFocusState private var accessibilityFocusedStage: ResearchFirstRunStage?

    var body: some View {
        Group {
            switch session.stage {
            case .environmentBrief:
                EnvironmentBriefContentView(
                    brief: projection.environmentBrief,
                    isRefreshing: false,
                    canScanAgain: false,
                    onScanAgain: {},
                    onComplete: onComplete,
                    onReviewPlan: reviewPlan
                )
            case .planReview:
                ResearchFirstRunPlanView(
                    projection: projection,
                    onBack: returnToBrief,
                    onUseHelm: onComplete,
                    onApply: applyReviewedPlan
                )
            case .verifiedProgress:
                ResearchFirstRunProgressView(
                    projection: projection,
                    onViewReceipt: viewActionReceipt
                )
            case .actionReceipt:
                ResearchFirstRunReceiptView(
                    projection: projection,
                    summaryCopied: summaryCopied,
                    onReviewPlan: reviewPlan,
                    onCopySummary: copySummary,
                    onComplete: onComplete
                )
            }
        }
        .id(session.stage)
        .accessibilityElement(children: .contain)
        .accessibilityFocused($accessibilityFocusedStage, equals: session.stage)
        .onAppear {
            moveAccessibilityFocus(to: session.stage, announce: false)
        }
        .onChange(of: session.stage) { stage in
            moveAccessibilityFocus(to: stage, announce: true)
        }
    }

    private func reviewPlan() {
        summaryCopied = false
        session.reviewPlan()
    }

    private func returnToBrief() {
        session.returnToBrief()
    }

    private func applyReviewedPlan() {
        session.applyReviewedPlan()
    }

    private func viewActionReceipt() {
        session.viewActionReceipt()
    }

    private func copySummary() {
        let summaryProjection = projection.summaryProjection
        let summary = L10n.App.FirstRun.Research.CopySummary.template.localized(with: [
            "status": localizedSummaryStatus(summaryProjection.status),
            "total": summaryProjection.totalActions,
            "verified": summaryProjection.verifiedActions,
            "network": localizedYesNo(summaryProjection.networkUsed),
            "authorization": localizedYesNo(
                summaryProjection.administratorAuthorizationUsed
            ),
            "os": summaryProjection.osFamily,
            "architecture": summaryProjection.architecture,
        ])
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        summaryCopied = pasteboard.setString(summary, forType: .string)
        if summaryCopied {
            HelmCore.shared.postAccessibilityAnnouncement(
                L10n.App.FirstRun.Research.CopySummary.copied.localized
            )
        }
    }

    private func moveAccessibilityFocus(
        to stage: ResearchFirstRunStage,
        announce: Bool
    ) {
        DispatchQueue.main.async {
            accessibilityFocusedStage = stage
            if announce {
                HelmCore.shared.postAccessibilityAnnouncement(
                    accessibilityAnnouncement(for: stage)
                )
            }
        }
    }

    private func accessibilityAnnouncement(for stage: ResearchFirstRunStage) -> String {
        switch stage {
        case .environmentBrief:
            return L10n.App.FirstRun.eyebrow.localized
        case .planReview:
            return L10n.App.FirstRun.Research.Plan.title.localized(with: [
                "manager": localizedManagerDisplayName(
                    projection.recommendation.target.identifier
                )
            ])
        case .verifiedProgress:
            return L10n.App.FirstRun.Research.Progress.title.localized
        case .actionReceipt:
            return L10n.App.FirstRun.Research.Receipt.title.localized
        }
    }

    private func localizedYesNo(_ value: Bool) -> String {
        value
            ? L10n.App.FirstRun.Research.yes.localized
            : L10n.App.FirstRun.Research.no.localized
    }

    private func localizedSummaryStatus(_ status: String) -> String {
        status == "verified"
            ? L10n.App.FirstRun.Research.verified.localized
            : L10n.Common.unknown.localized
    }
}

struct ResearchFirstRunUnavailableView: View {
    let onComplete: () -> Void

    var body: some View {
        VStack(spacing: 18) {
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 42, weight: .medium))
                .foregroundColor(HelmTheme.stateUnavailable)
                .accessibilityHidden(true)

            Text(L10n.App.FirstRun.Research.Unavailable.title.localized)
                .font(.system(.title, design: .rounded, weight: .semibold))
                .accessibilityAddTraits(.isHeader)

            Text(L10n.App.FirstRun.Research.Unavailable.detail.localized)
                .font(.body)
                .foregroundColor(HelmTheme.textSecondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 520)

            ResearchFirstRunSafetyNotice()

            Button(L10n.App.FirstRun.Action.useHelm.localized, action: onComplete)
                .buttonStyle(HelmPrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct ResearchFirstRunPlanView: View {
    let projection: ResearchFirstRunProjection
    let onBack: () -> Void
    let onUseHelm: () -> Void
    let onApply: () -> Void

    private var action: ResearchPlannedAction { projection.recommendation }
    private var failedManagerName: String {
        projection.environmentBrief.coverage.failedManagers.first
            .map(localizedManagerDisplayName)
            ?? L10n.Common.unknown.localized
    }

    var body: some View {
        ResearchFirstRunStageLayout(
            eyebrow: L10n.App.FirstRun.Research.Plan.eyebrow.localized,
            title: L10n.App.FirstRun.Research.Plan.title.localized(with: [
                "manager": localizedManagerDisplayName(action.target.identifier)
            ]),
            subtitle: L10n.App.FirstRun.Research.Plan.subtitle.localized,
            symbol: "wrench.and.screwdriver.fill",
            tint: HelmTheme.stateNeedsReview
        ) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .top, spacing: 18) {
                    observedCard
                    consequencesCard
                }

                VStack(alignment: .leading, spacing: 18) {
                    observedCard
                    consequencesCard
                }
            }
        } footer: {
            ResearchFirstRunSafetyNotice()
            Spacer(minLength: 8)
            Button(L10n.App.FirstRun.Research.Action.back.localized, action: onBack)
                .buttonStyle(HelmTertiaryButtonStyle())
            Button(L10n.App.FirstRun.Action.useHelm.localized, action: onUseHelm)
                .buttonStyle(HelmSecondaryButtonStyle())
            Button(L10n.App.FirstRun.Research.Action.apply.localized, action: onApply)
                .buttonStyle(HelmPrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
        }
    }

    private var observedCard: some View {
        ResearchFirstRunCard(
            title: L10n.App.FirstRun.Research.Plan.observed.localized,
            symbol: "scope"
        ) {
            Text(localizedResearchText(action.impactKey, arguments: action.impactArgs))
            Divider()
            Label(
                L10n.App.FirstRun.Research.Plan.partialCoverage.localized(with: [
                    "manager": failedManagerName
                ]),
                systemImage: "exclamationmark.triangle"
            )
            .foregroundColor(HelmTheme.stateUnavailable)
        }
        .frame(minWidth: 320)
    }

    private var consequencesCard: some View {
        ResearchFirstRunCard(
            title: L10n.App.FirstRun.Research.Plan.consequences.localized,
            symbol: "list.bullet.clipboard"
        ) {
            ResearchFirstRunFactRow(
                title: L10n.App.FirstRun.Research.Plan.network.localized,
                value: L10n.App.FirstRun.Research.Plan.notRequired.localized,
                symbol: "network.slash"
            )
            ResearchFirstRunFactRow(
                title: L10n.App.FirstRun.Research.Plan.authorization.localized,
                value: L10n.App.FirstRun.Research.Plan.notRequired.localized,
                symbol: "lock.open"
            )
            ResearchFirstRunFactRow(
                title: L10n.App.FirstRun.Research.Plan.verification.localized,
                value: localizedResearchFact(projection.expectedStateFact),
                symbol: "checkmark.seal"
            )
            ResearchFirstRunFactRow(
                title: L10n.App.FirstRun.Research.Plan.recovery.localized,
                value: localizedResearchText(action.recoveryLimitsKey),
                symbol: "arrow.uturn.backward.circle"
            )
        }
        .frame(minWidth: 320)
    }
}

private struct ResearchFirstRunProgressView: View {
    let projection: ResearchFirstRunProjection
    let onViewReceipt: () -> Void

    private let steps: [(String, String)] = [
        (L10n.App.FirstRun.Research.Progress.revalidate, "arrow.triangle.2.circlepath"),
        (L10n.App.FirstRun.Research.Progress.apply, "wrench.and.screwdriver"),
        (L10n.App.FirstRun.Research.Progress.verify, "checkmark.seal"),
        (L10n.App.FirstRun.Research.Progress.record, "doc.text"),
    ]

    var body: some View {
        ResearchFirstRunStageLayout(
            eyebrow: L10n.App.FirstRun.Research.Progress.eyebrow.localized,
            title: L10n.App.FirstRun.Research.Progress.title.localized,
            subtitle: projection.actionReceipt.summaryKey.localized(with: localizedArguments(
                projection.actionReceipt.summaryArgs
            )),
            symbol: "checkmark.circle.fill",
            tint: HelmTheme.stateHealthy
        ) {
            ResearchFirstRunCard(
                title: L10n.App.FirstRun.Research.Progress.details.localized,
                symbol: "list.number"
            ) {
                ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                    HStack(spacing: 12) {
                        ZStack {
                            Circle()
                                .fill(HelmTheme.stateHealthy.opacity(0.12))
                            Image(systemName: "checkmark")
                                .font(.caption.weight(.bold))
                                .foregroundColor(HelmTheme.stateHealthy)
                        }
                        .frame(width: 28, height: 28)
                        .accessibilityHidden(true)

                        Image(systemName: step.1)
                            .foregroundColor(HelmTheme.textSecondary)
                            .frame(width: 20)
                            .accessibilityHidden(true)

                        Text(step.0.localized(with: [
                            "manager": localizedManagerDisplayName(
                                projection.recommendation.target.identifier
                            )
                        ]))
                        .font(.body.weight(.medium))

                        Spacer()

                        Text(L10n.App.FirstRun.Research.verified.localized)
                            .font(.caption.weight(.semibold))
                            .foregroundColor(HelmTheme.stateHealthy)
                    }
                    .padding(.vertical, 7)
                    .accessibilityElement(children: .combine)

                    if index < steps.count - 1 {
                        Divider()
                    }
                }
            }
        } footer: {
            ResearchFirstRunSafetyNotice()
            Spacer(minLength: 8)
            Button(
                L10n.App.FirstRun.Research.Action.viewReceipt.localized,
                action: onViewReceipt
            )
            .buttonStyle(HelmPrimaryButtonStyle())
            .keyboardShortcut(.defaultAction)
        }
    }
}

private struct ResearchFirstRunReceiptView: View {
    let projection: ResearchFirstRunProjection
    let summaryCopied: Bool
    let onReviewPlan: () -> Void
    let onCopySummary: () -> Void
    let onComplete: () -> Void

    private var result: ResearchActionResult { projection.receiptResult }

    var body: some View {
        ResearchFirstRunStageLayout(
            eyebrow: L10n.App.FirstRun.Research.Receipt.eyebrow.localized,
            title: L10n.App.FirstRun.Research.Receipt.title.localized,
            subtitle: projection.actionReceipt.summaryKey.localized(with: localizedArguments(
                projection.actionReceipt.summaryArgs
            )),
            symbol: "checkmark.seal.fill",
            tint: HelmTheme.stateHealthy
        ) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .top, spacing: 18) {
                    resultCard
                    unchangedCard
                }

                VStack(alignment: .leading, spacing: 18) {
                    resultCard
                    unchangedCard
                }
            }
        } footer: {
            if summaryCopied {
                Label(
                    L10n.App.FirstRun.Research.CopySummary.copied.localized,
                    systemImage: "checkmark.circle.fill"
                )
                .font(.caption.weight(.semibold))
                .foregroundColor(HelmTheme.stateHealthy)
                .accessibilityElement(children: .combine)
            } else {
                ResearchFirstRunSafetyNotice()
            }
            Spacer(minLength: 8)
            Button(L10n.App.FirstRun.Research.Action.reviewPlan.localized, action: onReviewPlan)
                .buttonStyle(HelmTertiaryButtonStyle())
            Button(L10n.App.FirstRun.Research.Action.copySummary.localized, action: onCopySummary)
                .buttonStyle(HelmSecondaryButtonStyle())
            Button(L10n.App.FirstRun.Research.Action.openHelm.localized, action: onComplete)
                .buttonStyle(HelmPrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
        }
    }

    private var resultCard: some View {
        ResearchFirstRunCard(
            title: L10n.App.FirstRun.Research.Receipt.result.localized,
            symbol: "arrow.left.arrow.right"
        ) {
            ResearchFirstRunReceiptFact(
                title: L10n.App.FirstRun.Research.Receipt.before.localized,
                text: localizedResearchFact(result.before)
            )
            Divider()
            ResearchFirstRunReceiptFact(
                title: L10n.App.FirstRun.Research.Receipt.after.localized,
                text: localizedResearchFact(result.after),
                tint: HelmTheme.stateHealthy
            )
            Divider()
            ResearchFirstRunReceiptFact(
                title: L10n.App.FirstRun.Research.Receipt.recovery.localized,
                text: localizedResearchFact(result.recoveryLimits)
            )
        }
        .frame(minWidth: 320)
    }

    private var unchangedCard: some View {
        ResearchFirstRunCard(
            title: L10n.App.FirstRun.Research.Receipt.unchanged.localized,
            symbol: "hand.raised"
        ) {
            ForEach(
                Array(projection.actionReceipt.unchangedState.enumerated()),
                id: \.offset
            ) { _, fact in
                Label(localizedResearchFact(fact), systemImage: "checkmark.circle")
                    .foregroundColor(HelmTheme.textSecondary)
            }
        }
        .frame(minWidth: 320)
    }
}

private struct ResearchFirstRunStageLayout<Content: View, Footer: View>: View {
    let eyebrow: String
    let title: String
    let subtitle: String
    let symbol: String
    let tint: Color
    @ViewBuilder let content: Content
    @ViewBuilder let footer: Footer

    init(
        eyebrow: String,
        title: String,
        subtitle: String,
        symbol: String,
        tint: Color,
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) {
        self.eyebrow = eyebrow
        self.title = title
        self.subtitle = subtitle
        self.symbol = symbol
        self.tint = tint
        self.content = content()
        self.footer = footer()
    }

    var body: some View {
        GeometryReader { geometry in
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    HStack(spacing: 20) {
                        ZStack {
                            Circle()
                                .fill(tint.opacity(0.12))
                            Circle()
                                .stroke(tint.opacity(0.34), lineWidth: 1)
                            Image(systemName: symbol)
                                .font(.system(size: 30, weight: .medium))
                                .foregroundColor(tint)
                        }
                        .frame(width: 78, height: 78)
                        .accessibilityHidden(true)

                        VStack(alignment: .leading, spacing: 7) {
                            Text(eyebrow)
                                .font(.caption.weight(.bold))
                                .tracking(1.05)
                                .foregroundColor(HelmTheme.blue500)
                            Text(title)
                                .font(
                                    .system(
                                        .largeTitle,
                                        design: .rounded,
                                        weight: .semibold
                                    )
                                )
                                .foregroundColor(HelmTheme.textPrimary)
                                .fixedSize(horizontal: false, vertical: true)
                                .accessibilityAddTraits(.isHeader)
                            Text(subtitle)
                                .font(.title3)
                                .foregroundColor(HelmTheme.textSecondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }

                    content
                        .frame(maxWidth: .infinity, alignment: .topLeading)

                    Spacer(minLength: 0)

                    HStack(spacing: 10) {
                        footer
                    }
                }
                .padding(.horizontal, 40)
                .padding(.vertical, 34)
                .frame(
                    maxWidth: 980,
                    minHeight: geometry.size.height,
                    alignment: .topLeading
                )
                .frame(maxWidth: .infinity, alignment: .top)
            }
        }
    }
}

private struct ResearchFirstRunCard<Content: View>: View {
    let title: String
    let symbol: String
    @ViewBuilder let content: Content

    init(
        title: String,
        symbol: String,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.symbol = symbol
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(title, systemImage: symbol)
                .font(.headline)
                .foregroundColor(HelmTheme.textPrimary)
            content
                .font(.body)
                .foregroundColor(HelmTheme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .background(
            RoundedRectangle(cornerRadius: HelmMetrics.radiusCard, style: .continuous)
                .fill(HelmTheme.surfacePanel)
                .overlay(
                    RoundedRectangle(cornerRadius: HelmMetrics.radiusCard, style: .continuous)
                        .strokeBorder(HelmTheme.borderSubtle, lineWidth: 1)
                )
        )
    }
}

private struct ResearchFirstRunFactRow: View {
    let title: String
    let value: String
    let symbol: String

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: symbol)
                .foregroundColor(HelmTheme.blue500)
                .frame(width: 18)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.caption.weight(.semibold))
                    .foregroundColor(HelmTheme.textSecondary)
                Text(value)
                    .font(.callout)
                    .foregroundColor(HelmTheme.textPrimary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

private struct ResearchFirstRunReceiptFact: View {
    let title: String
    let text: String
    var tint: Color = HelmTheme.textPrimary

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundColor(HelmTheme.textSecondary)
            Text(text)
                .font(.callout.weight(.medium))
                .foregroundColor(tint)
                .fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
    }
}

private struct ResearchFirstRunSafetyNotice: View {
    var body: some View {
        Label(
            L10n.App.FirstRun.Research.previewNotice.localized,
            systemImage: "testtube.2"
        )
        .font(.caption.weight(.medium))
        .foregroundColor(HelmTheme.textSecondary)
        .accessibilityElement(children: .combine)
    }
}

private func localizedResearchFact(_ fact: ResearchLocalizedFact) -> String {
    localizedResearchText(fact.localizationKey, arguments: fact.localizationArgs)
}

private func localizedResearchText(
    _ key: String,
    arguments: [String: String] = [:]
) -> String {
    key.localized(with: localizedArguments(arguments))
}

private func localizedArguments(_ arguments: [String: String]) -> [String: Any] {
    arguments.reduce(into: [:]) { result, pair in
        result[pair.key] = pair.key == "manager"
            ? localizedManagerDisplayName(pair.value)
            : pair.value
    }
}
