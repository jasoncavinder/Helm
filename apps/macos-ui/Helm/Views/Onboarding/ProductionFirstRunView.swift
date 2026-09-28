import SwiftUI

/// Real local evidence, deliberately separate from synthetic research plans/receipts.
struct ProductionFirstRunView: View {
    @ObservedObject var entry: FirstRunEntryController
    @ObservedObject private var localization = LocalizationManager.shared
    let onRetry: () -> Void

    var body: some View {
        Group {
            switch entry.phase {
            case .legal:
                EnvironmentBriefLegalGateView(onAccept: entry.acceptTerms)
            case .brief:
                evidenceContent
            case .reviewingRepair, .reviewRepair, .applyingRepair, .readingRepairReceipt, .repairReceipt, .repairUnavailable:
                ProductionFirstRunRepairView(entry: entry)
            case .failed:
                VStack(spacing: 20) {
                    Image(systemName: "exclamationmark.triangle")
                        .font(.largeTitle)
                        .foregroundColor(HelmTheme.blue500)
                        .accessibilityHidden(true)
                    Text("app.first_run.entry.unavailable".localized)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: 520)
                    Button("app.first_run.entry.retry".localized, action: onRetry)
                        .buttonStyle(HelmPrimaryButtonStyle())
                }
            default:
                VStack(spacing: 20) {
                    ProgressView()
                    Text("app.first_run.entry.preparing".localized)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(HelmTheme.surfaceBase)
    }

    private var evidenceContent: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text(L10n.App.FirstRun.eyebrow.localized)
                .font(.caption.weight(.bold))
                .foregroundColor(HelmTheme.blue500)
            Text("app.first_run.entry.title".localized)
                .font(.system(.largeTitle, design: .rounded, weight: .semibold))
                .accessibilityAddTraits(.isHeader)
            Text("app.first_run.entry.local_only".localized)
                .foregroundColor(HelmTheme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    ForEach(entry.observation?.managers ?? []) { manager in
                        HStack(alignment: .top, spacing: 16) {
                            Text(localizedManagerDisplayName(manager.managerId))
                                .font(.headline)
                                .frame(width: 190, alignment: .leading)
                            VStack(alignment: .leading, spacing: 4) {
                                Text("app.first_run.entry.candidates".localized(with: [
                                    "count": manager.candidatePaths.count
                                ]))
                                if let cached = manager.cachedDetection, cached.installed {
                                    Text("app.first_run.entry.cached".localized)
                                }
                                if manager.candidateScanStatus != "complete" {
                                    Text("app.first_run.entry.partial".localized)
                                }
                                if manager.configuredEnabled == false {
                                    Text("app.first_run.entry.disabled".localized)
                                }
                            }
                            .foregroundColor(HelmTheme.textSecondary)
                            Spacer(minLength: 0)
                        }
                        .padding(14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(HelmTheme.surfaceElevated, in: RoundedRectangle(cornerRadius: 12))
                        .accessibilityElement(children: .combine)
                    }
                }
            }
            Text("app.first_run.entry.continue_disclosure".localized)
                .font(.callout)
                .foregroundColor(HelmTheme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 12) {
                Button(L10n.App.FirstRun.Action.useHelm.localized, action: entry.continueToHelm)
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                Button(L10n.App.FirstRun.Action.scanAgain.localized, action: entry.observe)
                    .buttonStyle(HelmSecondaryButtonStyle())
                if entry.canReviewRepair {
                    Button("app.first_run.repair.review".localized, action: entry.reviewRepair)
                        .buttonStyle(HelmSecondaryButtonStyle())
                }
                Spacer()
            }
        }
        .padding(32)
    }
}

private struct ProductionFirstRunRepairView: View {
    @ObservedObject var entry: FirstRunEntryController

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text((entry.phase == .repairReceipt ? "app.first_run.repair.receipt_title" : "app.first_run.repair.review").localized)
                .font(.caption.weight(.bold))
                .foregroundColor(HelmTheme.blue500)
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    switch entry.phase {
                    case .reviewingRepair, .applyingRepair, .readingRepairReceipt:
                        ProgressView()
                        Text("app.first_run.repair.working".localized)
                    case .repairUnavailable:
                        heading("app.first_run.repair.unavailable_title")
                        Text((entry.repairMayHaveChanged ? "app.first_run.repair.unknown" : "app.first_run.repair.load_failed").localized)
                    case .repairReceipt:
                        if let receipt = entry.repairReceipt { receiptCard(receipt) }
                    case .reviewRepair:
                        if let plan = entry.repairReview?.plan {
                            heading("app.first_run.repair.title")
                            Text("app.first_run.repair.scope".localized)
                            pathRow("app.first_run.repair.old_path", plan.before.selectedExecutablePath)
                            pathRow("app.first_run.repair.check_path", plan.executable.path)
                            Text("app.first_run.repair.verification".localized)
                            Text("app.first_run.repair.limits".localized)
                                .foregroundColor(HelmTheme.textSecondary)
                        } else {
                            heading("app.first_run.repair.no_plan")
                        }
                        if let receipts = entry.repairReview?.receipts, !receipts.isEmpty {
                            Text("app.first_run.repair.history".localized)
                                .font(.headline)
                                .accessibilityAddTraits(.isHeader)
                            ForEach(receipts) { receiptCard($0) }
                        }
                    default:
                        EmptyView()
                    }
                }
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            if entry.phase == .reviewRepair, entry.repairReview?.plan != nil {
                Button("app.first_run.repair.confirm".localized, action: entry.confirmRepair)
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
            if entry.canContinue {
                Text("app.first_run.entry.continue_disclosure".localized)
                    .font(.callout)
                    .foregroundColor(HelmTheme.textSecondary)
                Button(L10n.App.FirstRun.Action.useHelm.localized, action: entry.continueToHelm)
                    .buttonStyle(HelmPrimaryButtonStyle())
            }
            if [.reviewRepair, .repairReceipt, .repairUnavailable].contains(entry.phase) {
                HStack(spacing: 12) {
                    Button("app.first_run.repair.back".localized, action: entry.returnToBrief)
                        .buttonStyle(HelmSecondaryButtonStyle())
                    if entry.phase == .repairUnavailable {
                        Button("app.first_run.repair.saved_results".localized, action: entry.reviewRepair)
                            .buttonStyle(HelmSecondaryButtonStyle())
                    }
                }
            }
        }
        .padding(32)
        .frame(maxWidth: 780, maxHeight: .infinity, alignment: .leading)
    }

    private func heading(_ key: String) -> some View {
        Text(key.localized)
            .font(.system(.title, design: .rounded, weight: .semibold))
            .accessibilityAddTraits(.isHeader)
    }

    private func pathRow(_ key: String, _ path: String) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(key.localized).font(.headline)
            Text(path).font(.system(.body, design: .monospaced)).textSelection(.enabled)
        }
    }

    private func receiptCard(_ receipt: FirstRunRepairReceipt) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(receipt.titleKey.localized).font(.headline).accessibilityAddTraits(.isHeader)
            pathRow("app.first_run.repair.check_path", receipt.checkedPath)
            if let version = receipt.observedVersion {
                Text("app.first_run.repair.version".localized(with: ["version": version]))
            }
            Text("app.first_run.repair.limits".localized).foregroundColor(HelmTheme.textSecondary)
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(HelmTheme.surfaceElevated, in: RoundedRectangle(cornerRadius: 14))
    }
}
