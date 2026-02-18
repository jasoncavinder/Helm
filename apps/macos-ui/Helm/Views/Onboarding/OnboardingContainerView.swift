import SwiftUI

enum OnboardingStep: Int, CaseIterable {
    case welcome = 0
    case detection = 1
    case configure = 2
}

struct OnboardingContainerView: View {
    @ObservedObject var core = HelmCore.shared
    let onComplete: () -> Void

    @State private var currentStep: OnboardingStep = .welcome

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                ForEach(OnboardingStep.allCases, id: \.rawValue) { step in
                    Circle()
                        .fill(step == currentStep ? Color.accentColor : Color.gray.opacity(0.3))
                        .frame(width: 8, height: 8)
                }
            }
            .padding(.top, 16)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Step \(currentStep.rawValue + 1) of \(OnboardingStep.allCases.count)")

            Group {
                switch currentStep {
                case .welcome:
                    OnboardingWelcomeView {
                        currentStep = .detection
                    }
                case .detection:
                    OnboardingDetectionView {
                        currentStep = .configure
                    }
                case .configure:
                    OnboardingConfigureView(onFinish: onComplete)
                }
            }
            .frame(maxHeight: .infinity)
        }
    }
}
