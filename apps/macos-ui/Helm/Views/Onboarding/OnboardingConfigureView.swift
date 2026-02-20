import SwiftUI

struct OnboardingConfigureView: View {
    @ObservedObject var core = HelmCore.shared
    let onFinish: () -> Void

    private var detectedManagers: [ManagerInfo] {
        ManagerInfo.all.filter { manager in
            let status = core.managerStatuses[manager.id]
            let isImplemented = status?.isImplemented ?? manager.isImplemented
            return isImplemented && (status?.detected ?? false)
        }
    }

    var body: some View {
        VStack(spacing: 16) {
            Text(L10n.App.Onboarding.Configure.title.localized)
                .font(.headline)
                .padding(.top, 16)

            Text(L10n.App.Onboarding.Configure.subtitle.localized)
                .font(.subheadline)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 24)

            if detectedManagers.isEmpty {
                VStack(spacing: 8) {
                    Spacer()
                    Text(L10n.App.Onboarding.Configure.noneDetected.localized)
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                    Text(L10n.App.Onboarding.Configure.installLater.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .multilineTextAlignment(.center)
                    Spacer()
                }
            } else {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(detectedManagers) { manager in
                            let status = core.managerStatuses[manager.id]
                            let enabled = status?.enabled ?? true

                            HStack(spacing: 10) {
                                Circle()
                                    .fill(enabled ? Color.green : Color.gray)
                                    .frame(width: 8, height: 8)

                                HStack(spacing: 6) {
                                    Text(manager.displayName)
                                        .font(.subheadline)
                                        .fontWeight(.medium)
                                        .lineLimit(1)

                                    if let version = status?.version, !version.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                                        Text("·")
                                            .font(.caption2)
                                            .foregroundColor(.secondary)
                                        Text(L10n.Common.version.localized(with: ["version": version]))
                                            .font(.caption2)
                                            .foregroundColor(.secondary)
                                            .lineLimit(1)
                                    }
                                }

                                Spacer()

                                Text(enabled ? L10n.Common.enabled.localized : L10n.Common.disabled.localized)
                                    .font(.caption2)
                                    .foregroundColor(.secondary)

                                Toggle("", isOn: Binding(
                                    get: { enabled },
                                    set: { _ in
                                        core.setManagerEnabled(manager.id, enabled: !enabled)
                                    }
                                ))
                                .toggleStyle(.switch)
                                .scaleEffect(0.7)
                                .labelsHidden()
                                .accessibilityLabel(manager.displayName)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)

                            Divider()
                                .padding(.leading, 44)
                        }
                    }
                }
            }

            Spacer()

            Button(action: onFinish) {
                Text(L10n.App.Onboarding.Detection.continue.localized)
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
            }
            .buttonStyle(HelmPrimaryButtonStyle())
            .padding(.horizontal, 40)
            .padding(.bottom, 32)
        }
    }
}
