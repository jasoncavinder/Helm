import SwiftUI

struct OnboardingSettingsView: View {
    @ObservedObject var core = HelmCore.shared
    @ObservedObject var localization = LocalizationManager.shared
    let onFinish: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            Text(L10n.App.Onboarding.Settings.title.localized)
                .font(.headline)
                .padding(.top, 16)

            Text(L10n.App.Onboarding.Settings.subtitle.localized)
                .font(.subheadline)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 24)

            VStack(spacing: 0) {
                // Safe Mode
                settingRow(
                    label: L10n.App.Settings.Label.safeMode.localized,
                    description: L10n.App.Onboarding.Settings.safeModeDescription.localized
                ) {
                    Toggle("", isOn: Binding(
                        get: { core.safeModeEnabled },
                        set: { core.setSafeMode($0) }
                    ))
                    .toggleStyle(.switch)
                    .scaleEffect(0.7)
                    .labelsHidden()
                }

                Divider().padding(.leading, 16)

                // Auto Clean Kegs
                settingRow(
                    label: L10n.App.Settings.Label.autoCleanKegs.localized,
                    description: L10n.App.Onboarding.Settings.autoCleanDescription.localized
                ) {
                    Toggle("", isOn: Binding(
                        get: { core.homebrewKegAutoCleanupEnabled },
                        set: { core.setHomebrewKegAutoCleanup($0) }
                    ))
                    .toggleStyle(.switch)
                    .scaleEffect(0.7)
                    .labelsHidden()
                }

                Divider().padding(.leading, 16)

                // Language
                HStack {
                    Text(L10n.App.Settings.Label.language.localized)
                        .font(.subheadline)
                        .fontWeight(.medium)
                    Spacer()
                    Picker("", selection: $localization.currentLocale) {
                        Text(L10n.App.Settings.Label.systemDefaultWithEnglish.localized).tag("en")
                        Text(L10n.App.Settings.Label.spanish.localized).tag("es")
                        Text(L10n.App.Settings.Label.german.localized).tag("de")
                        Text(L10n.App.Settings.Label.french.localized).tag("fr")
                        Text(L10n.App.Settings.Label.portugueseBrazilian.localized).tag("pt-BR")
                        Text(L10n.App.Settings.Label.japanese.localized).tag("ja")
                    }
                    .labelsHidden()
                    .frame(width: 180)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
            }

            Spacer()

            Button(action: onFinish) {
                Text(L10n.App.Onboarding.Configure.finishSetup.localized)
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
            }
            .buttonStyle(HelmPrimaryButtonStyle())
            .padding(.horizontal, 40)
            .padding(.bottom, 32)
        }
    }

    private func settingRow<Control: View>(
        label: String,
        description: String,
        @ViewBuilder control: () -> Control
    ) -> some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(label)
                    .font(.subheadline)
                    .fontWeight(.medium)
                Text(description)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            Spacer()
            control()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }
}
