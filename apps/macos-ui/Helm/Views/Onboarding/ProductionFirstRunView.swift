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
                Spacer()
            }
        }
        .padding(32)
    }
}
