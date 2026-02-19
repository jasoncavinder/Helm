import SwiftUI

struct OnboardingDetectionView: View {
    @ObservedObject var core = HelmCore.shared
    let onContinue: () -> Void

    @State private var hasTriggeredDetection = false

    private var detectionComplete: Bool {
        hasTriggeredDetection && !core.onboardingDetectionInProgress
    }

    private var foundManagers: [ManagerInfo] {
        ManagerInfo.all.filter { manager in
            let status = core.managerStatuses[manager.id]
            let isImplemented = status?.isImplemented ?? manager.isImplemented
            return isImplemented && status?.detected == true
        }
    }

    var body: some View {
        VStack(spacing: 16) {
            Text(L10n.App.Onboarding.Detection.title.localized)
                .font(.headline)
                .padding(.top, 16)

            if detectionComplete {
                if foundManagers.isEmpty {
                    Text(L10n.App.Onboarding.Detection.noneDetected.localized)
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                } else {
                    Text(L10n.App.Onboarding.Detection.foundCount.localized(with: ["count": foundManagers.count]))
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                }
            } else {
                HStack(spacing: 8) {
                    ProgressView()
                        .scaleEffect(0.7)
                    Text(L10n.App.Onboarding.Detection.scanning.localized)
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                }
            }

            if foundManagers.isEmpty && !detectionComplete {
                Spacer()
            } else {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(foundManagers) { manager in
                            let status = core.managerStatuses[manager.id]
                            FoundManagerRow(manager: manager, status: status)
                            if manager.id != foundManagers.last?.id {
                                Divider()
                                    .padding(.leading, 44)
                            }
                        }
                    }
                }
            }

            Spacer()

            Button(action: onContinue) {
                Text(L10n.App.Onboarding.Detection.`continue`.localized)
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
            }
            .buttonStyle(HelmPrimaryButtonStyle())
            .disabled(!detectionComplete)
            .padding(.horizontal, 40)
            .padding(.bottom, 32)
        }
        .onAppear {
            if !hasTriggeredDetection {
                core.triggerOnboardingDetectionRefresh()
                hasTriggeredDetection = true
            }
        }
    }
}

private struct FoundManagerRow: View {
    let manager: ManagerInfo
    let status: ManagerStatus?

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "checkmark.circle.fill")
                .foregroundColor(.green)
                .frame(width: 20, height: 20)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 2) {
                Text(manager.displayName)
                    .font(.subheadline)
                    .fontWeight(.medium)

                if let version = status?.version, !version.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    Text(L10n.Common.version.localized(with: ["version": version]))
                        .font(.caption2)
                        .foregroundColor(.secondary)
                } else {
                    Text(L10n.Common.detected.localized)
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
            }

            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .accessibilityElement(children: .combine)
    }
}
