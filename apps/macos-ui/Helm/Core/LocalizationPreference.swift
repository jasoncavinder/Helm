import Foundation

struct LocalizationPreferenceStore {
    static let systemSelection = "system"
    static let preferenceKey = "user_locale_preference_v2"
    static let legacyPreferenceKey = "user_locale_preference"
    static let supportedSelections: Set<String> = [
        systemSelection,
        "en",
        "es",
        "de",
        "fr",
        "pt-BR",
        "ja",
        "hu",
    ]

    let defaults: UserDefaults

    func initialSelection() -> String {
        // The legacy key conflated System Default with English and did not record
        // whether a value was user-selected. Reset it once to the truthful default.
        defaults.removeObject(forKey: Self.legacyPreferenceKey)

        guard
            let stored = defaults.string(forKey: Self.preferenceKey),
            Self.supportedSelections.contains(stored)
        else {
            defaults.set(Self.systemSelection, forKey: Self.preferenceKey)
            return Self.systemSelection
        }

        return stored
    }

    func save(_ selection: String) {
        defaults.set(Self.normalized(selection), forKey: Self.preferenceKey)
    }

    static func normalized(_ selection: String) -> String {
        supportedSelections.contains(selection) ? selection : systemSelection
    }

    static func effectiveLocale(
        for selection: String,
        preferredLanguages: [String],
        fallback: String = "en"
    ) -> String {
        let normalizedSelection = normalized(selection)
        guard normalizedSelection == systemSelection else {
            return normalizedSelection
        }

        return preferredLanguages
            .lazy
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty } ?? fallback
    }
}
