import SwiftUI

struct OnboardingDetectionView: View {
    @ObservedObject var core = HelmCore.shared
    let onContinue: () -> Void

    @State private var hasTriggeredDetection = false

    private var detectionComplete: Bool {
        hasTriggeredDetection && !core.isRefreshing
    }

    var body: some View {
        VStack(spacing: 16) {
            Text("Detecting Package Managers")
                .font(.headline)
                .padding(.top, 16)

            Text("Scanning your system...")
                .font(.subheadline)
                .foregroundColor(.secondary)

            ScrollView {
                VStack(spacing: 0) {
                    ForEach(ManagerInfo.implemented) { manager in
                        let status = core.managerStatuses[manager.id]
                        DetectionRow(
                            manager: manager,
                            status: status,
                            isRefreshing: core.isRefreshing,
                            hasTriggered: hasTriggeredDetection
                        )
                        Divider()
                            .padding(.leading, 44)
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
                core.triggerRefresh()
                hasTriggeredDetection = true
            }
        }
    }
}

private struct DetectionRow: View {
    let manager: ManagerInfo
    let status: ManagerStatus?
    let isRefreshing: Bool
    let hasTriggered: Bool

    var body: some View {
        HStack(spacing: 12) {
            Group {
                if let status = status, status.detected {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundColor(.green)
                } else if !isRefreshing && hasTriggered {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundColor(.secondary)
                } else if hasTriggered {
                    ProgressView()
                        .scaleEffect(0.6)
                } else {
                    Circle()
                        .fill(Color.gray.opacity(0.3))
                }
            }
            .frame(width: 20, height: 20)

            VStack(alignment: .leading, spacing: 2) {
                Text(manager.displayName)
                    .font(.subheadline)
                    .fontWeight(.medium)

                if let status = status, status.detected, let version = status.version {
                    Text("v\(version)")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                } else if !isRefreshing && hasTriggered {
                    Text("Not found")
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
