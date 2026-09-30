import SwiftUI

struct ProductionFirstRunEvidenceView: View {
    @ObservedObject var entry: FirstRunEntryController
    @ObservedObject private var localization = LocalizationManager.shared
    let evidence: FirstRunLocalEvidence
    @State private var showOtherSources = false

    private var presentation: FirstRunEvidencePresentation { .init(evidence) }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            header
            HStack(alignment: .top, spacing: 24) {
                VStack(alignment: .leading, spacing: 18) {
                    preferences
                    Text("app.first_run.entry.local_only".localized)
                        .font(.callout)
                        .foregroundColor(HelmTheme.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 0)
                    actions
                }
                .frame(width: 310, alignment: .leading)
                .frame(maxHeight: .infinity, alignment: .top)
                sources
            }
            .frame(maxHeight: .infinity, alignment: .top)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .accessibilityIdentifier("productionFirstRunEvidence")
    }

    private var header: some View {
        HStack(spacing: 24) {
            VStack(spacing: 8) {
                ZStack {
                    Circle().stroke(HelmTheme.blue500.opacity(0.16), lineWidth: 10)
                    Circle()
                        .stroke(
                            LinearGradient(colors: [HelmTheme.blue500.opacity(0.5), HelmTheme.blue500],
                                           startPoint: .topLeading, endPoint: .bottomTrailing),
                            style: StrokeStyle(lineWidth: 4, lineCap: .round)
                        )
                    Text("\(presentation.sourcesWithFiles)")
                        .font(.system(size: 34, weight: .semibold, design: .rounded).monospacedDigit())
                        .foregroundColor(HelmTheme.blue500)
                }
                .frame(width: 82, height: 82)
                .padding(5)
                Text("app.first_run.evidence.file_sources".localized)
                    .font(.caption)
                    .foregroundColor(HelmTheme.textSecondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(width: 112)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(countLabel("app.first_run.evidence.file_sources", presentation.sourcesWithFiles))

            GroupBox {
                Text("app.first_run.evidence.summary".localized)
                    .font(.callout)
                    .foregroundColor(HelmTheme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            } label: {
                Text("app.first_run.evidence.title".localized)
                    .font(.system(.title, design: .rounded, weight: .semibold))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("productionFirstRunEvidenceTitle")
            }
            .groupBoxStyle(ProductionFirstRunHeadingStyle())
            .frame(maxWidth: .infinity, alignment: .leading)

            Button(action: entry.observe) {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.bordered)
            .help(L10n.App.FirstRun.Action.scanAgain.localized)
            .accessibilityLabel(L10n.App.FirstRun.Action.scanAgain.localized)
            .accessibilityIdentifier("productionFirstRunScanAgain")
        }
    }

    private var preferences: some View {
        GroupBox("app.first_run.evidence.preferences".localized) {
            VStack(alignment: .leading, spacing: 8) {
                countRow("app.first_run.evidence.enabled", presentation.savedEnabled)
                Divider()
                countRow("app.first_run.evidence.disabled", presentation.savedDisabled)
                Text("app.first_run.evidence.unchanged".localized)
                    .font(.caption)
                    .foregroundColor(HelmTheme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(6)
        }
    }

    private var actions: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("app.first_run.entry.continue_disclosure".localized)
                .font(.caption)
                .foregroundColor(HelmTheme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
            if entry.canReviewRepair {
                Button("app.first_run.repair.review".localized, action: entry.reviewRepair)
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .accessibilityIdentifier("productionFirstRunReviewRepair")
            }
            Button(L10n.App.FirstRun.Action.useHelm.localized, action: entry.continueToHelm)
                .buttonStyle(HelmPrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
                .accessibilityIdentifier("productionFirstRunContinue")
        }
    }

    private var sources: some View {
        GroupBox(L10n.App.FirstRun.Section.sources.localized) {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if presentation.incompleteScans > 0 {
                        Text("app.first_run.evidence.partial".localized(with: ["count": presentation.incompleteScans]))
                            .font(.callout)
                            .foregroundColor(HelmTheme.stateNeedsReview)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.vertical, 10)
                    }
                    if presentation.observed.isEmpty {
                        Text("app.first_run.evidence.empty".localized)
                            .foregroundColor(HelmTheme.textSecondary)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.vertical, 12)
                    }
                    ForEach(presentation.observed) { manager in
                        ProductionFirstRunSourceRow(manager: manager)
                        Divider()
                    }
                    if !presentation.other.isEmpty {
                        DisclosureGroup(isExpanded: $showOtherSources) {
                            ForEach(presentation.other) { manager in
                                ProductionFirstRunSourceRow(manager: manager)
                                Divider()
                            }
                        } label: {
                            Text("app.first_run.evidence.other".localized(with: ["count": presentation.other.count]))
                                .font(.callout.weight(.medium))
                        }
                        .padding(.vertical, 12)
                    }
                }
                .padding(.horizontal, 8)
            }
            .accessibilityIdentifier("productionFirstRunSources")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func countRow(_ key: String, _ count: Int) -> some View {
        HStack(spacing: 12) {
            Text(key.localized).font(.callout)
            Spacer(minLength: 4)
            Text("\(count)").font(.callout.weight(.semibold).monospacedDigit())
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(countLabel(key, count))
    }

    private func countLabel(_ key: String, _ count: Int) -> String {
        L10n.App.FirstRun.Readiness.accessibilityLabel.localized(with: ["title": key.localized, "count": count])
    }
}

private struct ProductionFirstRunHeadingStyle: GroupBoxStyle {
    func makeBody(configuration: Configuration) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            configuration.label
            configuration.content
        }
    }
}

private struct ProductionFirstRunSourceRow: View {
    @ObservedObject private var localization = LocalizationManager.shared
    let manager: FirstRunLocalEvidence.Manager

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(localizedManagerDisplayName(manager.managerId)).font(.headline)
            Text(FirstRunEvidencePresentation.observationKey(manager).localized(with: ["count": manager.candidatePaths.count]))
                .foregroundColor(HelmTheme.textSecondary)
            if manager.cachedDetection?.installed == true {
                Text("app.first_run.entry.cached".localized).foregroundColor(HelmTheme.textSecondary)
            }
            if manager.candidateScanStatus == "partial", !manager.candidatePaths.isEmpty {
                Text("app.first_run.entry.partial".localized).foregroundColor(HelmTheme.stateNeedsReview)
            }
            if manager.configuredEnabled == false {
                Text("app.first_run.entry.disabled".localized).foregroundColor(HelmTheme.textSecondary)
            }
            if !manager.candidatePaths.isEmpty || manager.selectedExecutablePath != nil {
                DisclosureGroup("app.first_run.evidence.paths".localized) {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(manager.candidatePaths, id: \.self) { path in pathText(path) }
                        if let saved = manager.selectedExecutablePath {
                            Text("app.first_run.evidence.saved_path".localized).font(.caption.weight(.semibold))
                            pathText(saved)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, 6)
                }
                .accessibilityLabel("\("app.first_run.evidence.paths".localized): \(localizedManagerDisplayName(manager.managerId))")
                .accessibilityIdentifier("productionFirstRunPaths-\(manager.managerId)")
            }
        }
        .font(.callout)
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 12)
    }

    private func pathText(_ path: String) -> some View {
        Text(path)
            .font(.system(.caption, design: .monospaced))
            .textSelection(.enabled)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}
