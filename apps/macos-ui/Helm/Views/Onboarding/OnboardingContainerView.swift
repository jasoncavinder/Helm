import SwiftUI

enum OnboardingStep: String, CaseIterable {
    case welcome
    case license
    case detection
    case configure
    case settings
}

struct OnboardingContainerView: View {
    @ObservedObject var core = HelmCore.shared
    let onComplete: () -> Void

    @State private var currentStep: OnboardingStep = .welcome

    private var stepSequence: [OnboardingStep] {
        var steps: [OnboardingStep] = []
        if !core.hasCompletedOnboarding {
            steps.append(.welcome)
            if core.requiresLicenseTermsAcceptance {
                steps.append(.license)
            }
            steps.append(contentsOf: [.detection, .configure, .settings])
        } else if core.requiresLicenseTermsAcceptance {
            steps.append(.license)
        }
        return steps
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                ForEach(stepSequence, id: \.rawValue) { step in
                    Circle()
                        .fill(step == currentStep ? HelmTheme.actionPrimaryDefault : HelmTheme.borderSubtle.opacity(0.8))
                        .frame(width: 8, height: 8)
                }
            }
            .padding(.top, 16)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(
                L10n.App.Walkthrough.Control.stepIndicator.localized(with: [
                    "current": (stepSequence.firstIndex(of: currentStep) ?? 0) + 1,
                    "total": max(stepSequence.count, 1)
                ])
            )

            Group {
                switch currentStep {
                case .license:
                    OnboardingLicenseView(
                        onViewTerms: {
                            HelmSupport.openURL(HelmSupport.licenseTermsURL)
                        },
                        onAccept: {
                            let sequenceBeforeAcceptance = stepSequence
                            core.acceptCurrentLicenseTerms()
                            advance(from: .license, in: sequenceBeforeAcceptance)
                        }
                    )
                case .welcome:
                    OnboardingWelcomeView {
                        advanceFromCurrentStep()
                    }
                case .detection:
                    OnboardingDetectionView {
                        advanceFromCurrentStep()
                    }
                case .configure:
                    OnboardingConfigureView {
                        advanceFromCurrentStep()
                    }
                case .settings:
                    OnboardingSettingsView(onFinish: {
                        advanceFromCurrentStep()
                    })
                }
            }
            .frame(maxHeight: .infinity)
        }
        .onAppear {
            resetToFirstAvailableStep()
        }
        .onChange(of: core.hasCompletedOnboarding) { _ in
            alignCurrentStepToAvailableSequence()
        }
        .onChange(of: core.requiresLicenseTermsAcceptance) { _ in
            alignCurrentStepToAvailableSequence()
        }
    }

    private func advanceFromCurrentStep() {
        advance(from: currentStep, in: stepSequence)
    }

    private func advance(from step: OnboardingStep, in sequence: [OnboardingStep]) {
        guard let currentIndex = sequence.firstIndex(of: step) else {
            alignCurrentStepToAvailableSequence()
            return
        }

        let nextIndex = currentIndex + 1
        guard sequence.indices.contains(nextIndex) else {
            onComplete()
            return
        }
        currentStep = sequence[nextIndex]
    }

    private func alignCurrentStepToAvailableSequence() {
        guard let firstStep = stepSequence.first else {
            return
        }
        if !stepSequence.contains(currentStep) {
            currentStep = firstStep
        }
    }

    private func resetToFirstAvailableStep() {
        guard let firstStep = stepSequence.first else {
            return
        }
        currentStep = firstStep
    }
}

private struct OnboardingLicenseView: View {
    let onViewTerms: () -> Void
    let onAccept: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            Text(L10n.App.Onboarding.License.title.localized)
                .font(.headline)
                .padding(.top, 16)

            Text(L10n.App.Onboarding.License.subtitle.localized)
                .font(.subheadline)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 24)

            Text(
                L10n.App.Onboarding.License.version.localized(with: [
                    "version": HelmCore.currentLicenseTermsVersion
                ])
            )
            .font(.caption)
            .foregroundColor(.secondary)

            ScrollView {
                Text(L10n.App.Onboarding.License.summary.localized)
                    .font(.callout)
                    .foregroundColor(.primary)
                    .multilineTextAlignment(.leading)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(12)
                    .background(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .fill(HelmTheme.surfacePanel)
                            .overlay(
                                RoundedRectangle(cornerRadius: 10, style: .continuous)
                                    .strokeBorder(HelmTheme.borderSubtle.opacity(0.95), lineWidth: 0.8)
                            )
                    )
            }
            .frame(maxHeight: 220)
            .padding(.horizontal, 24)

            Spacer()

            HStack(spacing: 8) {
                Button(L10n.App.Legal.Action.viewTerms.localized) {
                    onViewTerms()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .helmPointer()

                Button(L10n.App.Onboarding.License.accept.localized) {
                    onAccept()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .helmPointer()
            }
            .padding(.horizontal, 40)
            .padding(.bottom, 32)
        }
    }
}
