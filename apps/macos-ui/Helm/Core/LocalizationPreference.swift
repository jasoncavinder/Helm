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
        if let stored = defaults.string(forKey: Self.preferenceKey) {
            let normalizedStored = Self.normalized(stored)
            if normalizedStored != stored {
                defaults.set(normalizedStored, forKey: Self.preferenceKey)
            }
            return normalizedStored
        }

        if let legacy = defaults.string(forKey: Self.legacyPreferenceKey) {
            let migrated = Self.migratedLegacySelection(legacy)
            defaults.set(migrated, forKey: Self.preferenceKey)
            defaults.removeObject(forKey: Self.legacyPreferenceKey)
            return migrated
        }

        defaults.set(Self.systemSelection, forKey: Self.preferenceKey)
        return Self.systemSelection
    }

    func save(_ selection: String) {
        defaults.set(Self.normalized(selection), forKey: Self.preferenceKey)
    }

    static func normalized(_ selection: String) -> String {
        supportedSelections.contains(selection) ? selection : systemSelection
    }

    static func migratedLegacySelection(_ legacySelection: String) -> String {
        let normalizedLegacySelection = normalized(legacySelection)
        return normalizedLegacySelection == "en" ? systemSelection : normalizedLegacySelection
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
