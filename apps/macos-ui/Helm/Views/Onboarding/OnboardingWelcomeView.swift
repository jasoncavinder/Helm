import SwiftUI

struct OnboardingWelcomeView: View {
    let onContinue: () -> Void

    var body: some View {
        VStack(spacing: 24) {
            Spacer()

            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 80, height: 80)
                .cornerRadius(16)
                .accessibilityHidden(true)

            VStack(spacing: 8) {
                Text(L10n.App.Onboarding.Welcome.title.localized)
                    .font(.title2)
                    .fontWeight(.bold)

                Text(L10n.App.Onboarding.Welcome.subtitle.localized)
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 32)
            }

            Spacer()

            Button(action: onContinue) {
                Text(L10n.App.Onboarding.Welcome.getStarted.localized)
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
