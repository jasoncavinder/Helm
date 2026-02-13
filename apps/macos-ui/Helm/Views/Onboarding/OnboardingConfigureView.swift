import SwiftUI

struct OnboardingConfigureView: View {
    @ObservedObject var core = HelmCore.shared
    let onFinish: () -> Void

    private var detectedManagers: [ManagerInfo] {
        ManagerInfo.implemented.filter { manager in
            let status = core.managerStatuses[manager.id]
            return status?.detected ?? false
        }
    }

    var body: some View {
        VStack(spacing: 16) {
            Text("Configure Managers")
                .font(.headline)
                .padding(.top, 16)

            Text("Choose which package managers\nHelm should manage.")
                .font(.subheadline)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 24)

            if detectedManagers.isEmpty {
                VStack(spacing: 8) {
                    Spacer()
                    Text("No package managers were detected.")
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                    Text("You can install managers later\nfrom the Managers tab.")
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

                                VStack(alignment: .leading, spacing: 1) {
                                    Text(manager.displayName)
                                        .font(.subheadline)
                                        .fontWeight(.medium)

                                    if let version = status?.version {
                                        Text("v\(version)")
                                            .font(.caption2)
                                            .foregroundColor(.secondary)
                                    }
                                }

                                Spacer()

                                Text(enabled ? "Enabled" : "Disabled")
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
                Text("Finish Setup")
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
            }
            .buttonStyle(.borderedProminent)
            .padding(.horizontal, 40)
            .padding(.bottom, 32)
        }
    }
}
