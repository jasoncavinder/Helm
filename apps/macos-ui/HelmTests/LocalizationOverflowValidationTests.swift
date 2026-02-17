import XCTest
import AppKit

final class LocalizationOverflowValidationTests: XCTestCase {
    private let locales = ["es", "fr", "de", "pt-BR", "ja"]

    // Mirrors SettingsPopoverView fixed widths.
    private let settingsPopoverWidth: CGFloat = 440
    private let settingsHorizontalPadding: CGFloat = 16
    private let languagePickerWidth: CGFloat = 260
    private let frequencyPickerWidth: CGFloat = 100

    private var repoRootURL: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // HelmTests
            .deletingLastPathComponent() // macos-ui
            .deletingLastPathComponent() // apps
            .deletingLastPathComponent() // repo root
    }

    private func width(for text: String, font: NSFont) -> CGFloat {
        let attributes: [NSAttributedString.Key: Any] = [.font: font]
        return ceil((text as NSString).size(withAttributes: attributes).width)
    }

    private func localeAppStrings(_ locale: String) throws -> [String: String] {
        let fileURL = repoRootURL
            .appendingPathComponent("locales")
            .appendingPathComponent(locale)
            .appendingPathComponent("app.json")
        let data = try Data(contentsOf: fileURL)
        return try JSONDecoder().decode([String: String].self, from: data)
    }

    func testLanguagePickerOptionsFitConfiguredWidthAcrossLocales() throws {
        let keys = [
            "app.settings.label.language.system_default_with_english",
            "app.settings.label.language.spanish",
            "app.settings.label.language.german",
            "app.settings.label.language.french",
            "app.settings.label.language.portuguese_brazilian",
            "app.settings.label.language.japanese",
        ]

        let optionFont = NSFont.systemFont(ofSize: 13)
        let maxTextWidth = languagePickerWidth - 30 // reserve room for picker affordance and padding

        for locale in locales {
            let strings = try localeAppStrings(locale)
            for key in keys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    width(for: text, font: optionFont),
                    maxTextWidth,
                    "Language picker option overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }
        }
    }

    func testFrequencyPickerOptionsFitConfiguredWidthAcrossLocales() throws {
        let keys = [
            "app.settings.frequency.every_15_min",
            "app.settings.frequency.every_30_min",
            "app.settings.frequency.every_1_hour",
            "app.settings.frequency.daily",
        ]

        let optionFont = NSFont.systemFont(ofSize: 13)
        let maxTextWidth = frequencyPickerWidth - 26

        for locale in locales {
            let strings = try localeAppStrings(locale)
            for key in keys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    width(for: text, font: optionFont),
                    maxTextWidth,
                    "Frequency picker option overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }
        }
    }

    func testSettingsToggleAndLabelStringsFitPopoverContentAcrossLocales() throws {
        let contentWidth = settingsPopoverWidth - (settingsHorizontalPadding * 2)
        let availableLabelWidth = contentWidth - 56 // reserve toggle control and spacing
        let labelFont = NSFont.systemFont(ofSize: 13)

        let keys = [
            "app.settings.label.language",
            "app.settings.label.auto_check",
            "app.settings.label.check_frequency",
            "app.settings.label.safe_mode",
            "app.settings.label.auto_clean_kegs",
            "app.settings.action.refresh_now",
            "app.settings.action.upgrade_all",
            "app.settings.action.reset",
            "app.settings.action.quit",
        ]

        for locale in locales {
            let strings = try localeAppStrings(locale)
            for key in keys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    width(for: text, font: labelFont),
                    availableLabelWidth,
                    "Settings label overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }
        }
    }
}
