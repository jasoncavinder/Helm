import SwiftUI

struct OnboardingDetectionView: View {
    @ObservedObject var core = HelmCore.shared
    let onContinue: () -> Void

    @State private var hasTriggeredDetection = false

    private var detectionComplete: Bool {
        hasTriggeredDetection && !core.onboardingDetectionInProgress
    }

    private var foundManagers: [ManagerInfo] {
        ManagerInfo.implemented.filter { manager in
            core.managerStatuses[manager.id]?.detected == true
        }
    }

    var body: some View {
        VStack(spacing: 16) {
            Text("Detecting Package Managers")
                .font(.headline)
                .padding(.top, 16)

            if detectionComplete {
                if foundManagers.isEmpty {
                    Text("No package managers were detected.")
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                } else {
                    Text("Found \(foundManagers.count) package manager\(foundManagers.count == 1 ? "" : "s")")
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                }
            } else {
                HStack(spacing: 8) {
                    ProgressView()
                        .scaleEffect(0.7)
                    Text("Scanning your system\u{2026}")
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
                Text("Continue")
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
            }
            .buttonStyle(.borderedProminent)
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

            VStack(alignment: .leading, spacing: 2) {
                Text(manager.displayName)
                    .font(.subheadline)
                    .fontWeight(.medium)

                if let version = status?.version, !version.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    Text("v\(version)")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                } else {
                    Text("Detected")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
            }

            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }
}
