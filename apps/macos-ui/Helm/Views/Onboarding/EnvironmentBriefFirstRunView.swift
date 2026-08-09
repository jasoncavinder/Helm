import SwiftUI

struct EnvironmentBriefFirstRunView: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var overviewState = HelmCore.shared.overviewState
    @ObservedObject private var presentationModel = HelmCore.shared.firstRunPresentationModel
    @State private var hasStartedDiscovery = false

    let onComplete: () -> Void

    private var stage: FirstRunPresentationStage {
        if core.requiresLicenseTermsAcceptance {
            return .legal
        }
        if overviewState.environmentBrief != nil {
            return .brief
        }
        return presentationModel.state?.stage ?? .discovering
    }

    private var previewColorScheme: ColorScheme? {
        switch EnvironmentBriefFirstRunConfiguration.previewAppearance() {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }

    var body: some View {
        Group {
            switch stage {
            case .legal:
                EnvironmentBriefLegalGateView(
                    onAccept: {
                        core.acceptCurrentLicenseTerms()
                    }
                )
            case .discovering:
                EnvironmentBriefDiscoveringView()
            case .brief:
                if let brief = overviewState.environmentBrief {
                    EnvironmentBriefContentView(
                        brief: brief,
                        isRefreshing: core.onboardingDetectionInProgress,
                        canScanAgain: EnvironmentBriefFixtureProvider.active() == nil,
                        onScanAgain: runDiscovery,
                        onComplete: onComplete
                    )
                } else {
                    EnvironmentBriefDiscoveringView()
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(HelmTheme.surfaceBase)
        .preferredColorScheme(previewColorScheme)
        .onAppear(perform: startDiscoveryIfNeeded)
        .onChange(of: core.requiresLicenseTermsAcceptance) { requiresAcceptance in
            if !requiresAcceptance {
                startDiscoveryIfNeeded()
            }
        }
    }

    private func startDiscoveryIfNeeded() {
        guard !core.requiresLicenseTermsAcceptance else { return }
        guard !hasStartedDiscovery else { return }
        hasStartedDiscovery = true
        runDiscovery()
    }

    private func runDiscovery() {
        // Fixture previews must remain deterministic and service-independent.
        guard EnvironmentBriefFixtureProvider.active() == nil else { return }
        core.triggerOnboardingDetectionRefresh()
    }
}

private struct EnvironmentBriefLegalGateView: View {
    let onAccept: () -> Void

    var body: some View {
        VStack(spacing: 24) {
            Spacer(minLength: 28)

            Image(systemName: "checkmark.shield")
                .font(.system(size: 44, weight: .medium))
                .foregroundColor(HelmTheme.blue500)
                .accessibilityHidden(true)

            VStack(spacing: 8) {
                Text(L10n.App.Onboarding.License.title.localized)
                    .font(.system(.largeTitle, design: .rounded, weight: .semibold))

                Text(L10n.App.Onboarding.License.subtitle.localized)
                    .font(.body)
                    .foregroundColor(HelmTheme.textSecondary)
                    .multilineTextAlignment(.center)

                Text(
                    L10n.App.Onboarding.License.version.localized(with: [
                        "version": HelmCore.currentLicenseTermsVersion
                    ])
                )
                .font(.caption)
                .foregroundColor(HelmTheme.textSecondary)
            }

            GroupBox {
                Text(L10n.App.Onboarding.License.summary.localized)
                    .font(.body)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(8)
            }
            .frame(maxWidth: 620)

            HStack(spacing: 10) {
                Button(L10n.App.Legal.Action.viewTerms.localized) {
                    HelmSupport.openURL(HelmSupport.licenseTermsURL)
                }
                .buttonStyle(HelmSecondaryButtonStyle())

                Button(L10n.App.Onboarding.License.accept.localized, action: onAccept)
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }

            Spacer(minLength: 28)
        }
        .padding(40)
    }
}

private struct EnvironmentBriefDiscoveringView: View {
    var body: some View {
        VStack(spacing: 20) {
            ProgressView()
                .controlSize(.large)

            Text(L10n.App.FirstRun.eyebrow.localized)
                .font(.caption.weight(.bold))
                .tracking(1.1)
                .foregroundColor(HelmTheme.blue500)

            Text(L10n.App.FirstRun.discovering.localized)
                .font(.system(.title, design: .rounded, weight: .semibold))

            EnvironmentBriefTrustStrip(observationClass: .localOnly)
        }
        .padding(40)
        .accessibilityElement(children: .combine)
    }
}

private struct EnvironmentBriefContentView: View {
    let brief: EnvironmentBrief
    let isRefreshing: Bool
    let canScanAgain: Bool
    let onScanAgain: () -> Void
    let onComplete: () -> Void

    private var summary: EnvironmentBriefPresentationSummary {
        .make(from: brief)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                header
                EnvironmentBriefTrustStrip(observationClass: brief.observationClass)

                ViewThatFits(in: .horizontal) {
                    HStack(alignment: .top, spacing: 16) {
                        systemGroup
                        sourcesGroup
                    }

                    VStack(alignment: .leading, spacing: 16) {
                        systemGroup
                        sourcesGroup
                    }
                }

                HStack(spacing: 10) {
                    Button(L10n.App.FirstRun.Action.useHelm.localized, action: onComplete)
                        .buttonStyle(HelmPrimaryButtonStyle())
                        .keyboardShortcut(.defaultAction)

                    if summary.kind != .current && canScanAgain {
                        Button(L10n.App.FirstRun.Action.scanAgain.localized, action: onScanAgain)
                            .buttonStyle(HelmSecondaryButtonStyle())
                            .disabled(isRefreshing)
                    }

                    if isRefreshing {
                        ProgressView()
                            .controlSize(.small)
                            .accessibilityLabel(L10n.App.FirstRun.discovering.localized)
                    }
                }
            }
            .frame(maxWidth: 980, alignment: .leading)
            .padding(.horizontal, 40)
            .padding(.vertical, 34)
            .frame(maxWidth: .infinity)
        }
    }

    private var header: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 30) {
                EnvironmentBriefCourseIndicator(summary: summary)
                    .frame(width: 156, height: 156)
                headerCopy
                Spacer(minLength: 0)
            }

            VStack(alignment: .leading, spacing: 18) {
                EnvironmentBriefCourseIndicator(summary: summary)
                    .frame(width: 132, height: 132)
                headerCopy
            }
        }
    }

    private var headerCopy: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.App.FirstRun.eyebrow.localized)
                .font(.caption.weight(.bold))
                .tracking(1.1)
                .foregroundColor(HelmTheme.blue500)

            Text(titleKey.localized)
                .font(.system(.largeTitle, design: .rounded, weight: .semibold))
                .foregroundColor(HelmTheme.textPrimary)
                .fixedSize(horizontal: false, vertical: true)

            Text(summaryText)
                .font(.title3)
                .foregroundColor(HelmTheme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var systemGroup: some View {
        GroupBox(L10n.App.FirstRun.Section.thisMac.localized) {
            HStack(spacing: 22) {
                EnvironmentBriefFact(
                    icon: "laptopcomputer",
                    title: "macOS",
                    value: brief.system.osVersion
                )
                EnvironmentBriefFact(
                    icon: "cpu",
                    title: L10n.App.FirstRun.architecture.localized,
                    value: architectureLabel
                )
                EnvironmentBriefFact(
                    icon: "terminal",
                    title: L10n.App.FirstRun.shell.localized,
                    value: brief.system.activeShell
                )
            }
            .padding(8)
        }
        .frame(minWidth: 300, maxWidth: 360)
    }

    private var sourcesGroup: some View {
        GroupBox(L10n.App.FirstRun.Section.sources.localized) {
            VStack(spacing: 0) {
                if displayedManagers.isEmpty && !hasCoverageExceptions {
                    Text(L10n.App.Onboarding.Detection.noneDetected.localized)
                        .font(.body)
                        .foregroundColor(HelmTheme.textSecondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 12)
                }

                ForEach(Array(displayedManagers.enumerated()), id: \.element.manager) { index, manager in
                    EnvironmentBriefManagerRow(observation: manager)
                    if index < displayedManagers.count - 1 || hasCoverageExceptions {
                        Divider()
                    }
                }

                EnvironmentBriefCoverageRows(coverage: brief.coverage)
            }
            .padding(.horizontal, 8)
        }
        .frame(maxWidth: .infinity)
    }

    private var displayedManagers: [EnvironmentBriefManagerObservation] {
        brief.discoveredManagers.filter(\.detected)
    }

    private var hasCoverageExceptions: Bool {
        !brief.coverage.failedManagers.isEmpty
            || !brief.coverage.cancelledManagers.isEmpty
            || !brief.coverage.deferredManagers.isEmpty
    }

    private var titleKey: String {
        switch summary.kind {
        case .mapping: return L10n.App.FirstRun.Title.mapping
        case .current: return L10n.App.FirstRun.Title.current
        case .cached: return L10n.App.FirstRun.Title.cached
        case .partial: return L10n.App.FirstRun.Title.partial
        case .serviceFailure: return L10n.App.FirstRun.Title.serviceFailure
        }
    }

    private var summaryText: String {
        let key: String
        switch summary.kind {
        case .mapping: key = L10n.App.FirstRun.Summary.mapping
        case .current: key = L10n.App.FirstRun.Summary.current
        case .cached: key = L10n.App.FirstRun.Summary.cached
        case .partial: key = L10n.App.FirstRun.Summary.partial
        case .serviceFailure: key = L10n.App.FirstRun.Summary.serviceFailure
        }
        return key.localized(with: [
            "ready": summary.readyManagerCount,
            "mapped": summary.mappedManagerCount,
            "total": summary.intendedManagerCount,
            "attention": summary.attentionCount
        ])
    }

    private var architectureLabel: String {
        switch brief.system.architecture {
        case .arm64: return L10n.App.FirstRun.architectureAppleSilicon.localized
        case .x86_64: return L10n.App.FirstRun.architectureIntel.localized
        }
    }
}

private struct EnvironmentBriefCourseIndicator: View {
    let summary: EnvironmentBriefPresentationSummary

    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor

    private var tint: Color {
        switch summary.kind {
        case .mapping: return HelmTheme.stateRunning
        case .current: return HelmTheme.stateHealthy
        case .cached, .partial: return HelmTheme.stateAttention
        case .serviceFailure: return HelmTheme.stateError
        }
    }

    private var symbol: String {
        switch summary.kind {
        case .mapping: return "scope"
        case .current: return "checkmark"
        case .cached: return "clock.arrow.circlepath"
        case .partial: return "exclamationmark"
        case .serviceFailure: return "exclamationmark.triangle"
        }
    }

    var body: some View {
        ZStack {
            Circle()
                .stroke(tint.opacity(0.16), lineWidth: 13)

            Circle()
                .trim(from: 0, to: max(summary.completionFraction, 0.04))
                .stroke(
                    tint,
                    style: StrokeStyle(
                        lineWidth: 13,
                        lineCap: .round,
                        dash: differentiateWithoutColor ? [5, 4] : []
                    )
                )
                .rotationEffect(.degrees(-90))

            VStack(spacing: 6) {
                Image(systemName: symbol)
                    .font(.system(size: 30, weight: .medium))
                    .foregroundColor(tint)

                Text(summary.completionFraction, format: .percent.precision(.fractionLength(0)))
                    .font(.caption.weight(.bold).monospacedDigit())
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(L10n.App.FirstRun.Section.sources.localized)
        .accessibilityValue("\(Int((summary.completionFraction * 100).rounded()))%")
    }
}

private struct EnvironmentBriefTrustStrip: View {
    let observationClass: EnvironmentBriefObservationClass

    var body: some View {
        HStack(spacing: 18) {
            trustLabel(L10n.App.FirstRun.Trust.local, symbol: "internaldrive")
            trustLabel(L10n.App.FirstRun.Trust.noChanges, symbol: "hand.raised")
            trustLabel(
                observationClass == .localOnly
                    ? L10n.App.FirstRun.Trust.noNetwork
                    : L10n.App.FirstRun.Trust.disclosedNetwork,
                symbol: observationClass == .localOnly ? "network.slash" : "network"
            )
        }
        .font(.callout)
        .foregroundColor(HelmTheme.textSecondary)
        .accessibilityElement(children: .combine)
    }

    private func trustLabel(_ key: String, symbol: String) -> some View {
        Label(key.localized, systemImage: symbol)
    }
}

private struct EnvironmentBriefFact: View {
    let icon: String
    let title: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Label(title, systemImage: icon)
                .font(.caption)
                .foregroundColor(HelmTheme.textSecondary)
            Text(value)
                .font(.headline.monospacedDigit())
                .lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct EnvironmentBriefManagerRow: View {
    let observation: EnvironmentBriefManagerObservation

    private var status: (title: String, symbol: String, tint: Color) {
        if observation.eligibility != .eligible {
            return (L10n.App.FirstRun.Status.protected.localized, "lock.shield", HelmTheme.stateAttention)
        }
        switch observation.managementState {
        case .ready where observation.freshness == .cached:
            return (L10n.App.FirstRun.Status.cached.localized, "clock", HelmTheme.stateAttention)
        case .ready:
            return (L10n.App.FirstRun.Status.ready.localized, "checkmark.circle.fill", HelmTheme.stateHealthy)
        case .setupRequired:
            return (L10n.App.FirstRun.Status.setupRequired.localized, "wrench.and.screwdriver", HelmTheme.stateAttention)
        case .multipleInstancesAttention:
            return (L10n.App.FirstRun.Status.multipleInstances.localized, "square.stack.3d.up", HelmTheme.stateAttention)
        case .detectedUnmanageable:
            return (L10n.App.FirstRun.Status.protected.localized, "lock.shield", HelmTheme.stateAttention)
        case .notInstalled, .unknown:
            return (L10n.App.FirstRun.Status.reviewing.localized, "ellipsis.circle", HelmTheme.textSecondary)
        }
    }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: status.symbol)
                .foregroundColor(status.tint)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                Text(localizedManagerDisplayName(observation.manager))
                    .font(.body.weight(.medium))
            }

            Spacer()

            Text(status.title)
                .font(.caption.weight(.semibold))
                .foregroundColor(status.tint)
        }
        .padding(.vertical, 10)
        .accessibilityElement(children: .combine)
    }
}

private struct EnvironmentBriefCoverageRows: View {
    let coverage: EnvironmentBriefCoverage

    var body: some View {
        coverageRows(coverage.failedManagers, statusKey: L10n.App.FirstRun.Status.failed, symbol: "xmark.circle")
        coverageRows(
            coverage.cancelledManagers,
            statusKey: L10n.App.FirstRun.Status.cancelled,
            symbol: "stop.circle"
        )
        coverageRows(
            coverage.deferredManagers,
            statusKey: L10n.App.FirstRun.Status.deferred,
            symbol: "clock.arrow.circlepath"
        )
    }

    @ViewBuilder
    private func coverageRows(_ managers: [String], statusKey: String, symbol: String) -> some View {
        ForEach(managers, id: \.self) { manager in
            HStack(spacing: 12) {
                Image(systemName: symbol)
                    .foregroundColor(HelmTheme.stateAttention)
                    .frame(width: 20)
                Text(localizedManagerDisplayName(manager))
                    .font(.body.weight(.medium))
                Spacer()
                Text(statusKey.localized)
                    .font(.caption.weight(.semibold))
                    .foregroundColor(HelmTheme.stateAttention)
            }
            .padding(.vertical, 10)
            .accessibilityElement(children: .combine)
        }
    }
}
