import XCTest
import AppKit

final class LocalizationOverflowValidationTests: XCTestCase {
    private let locales = ["es", "fr", "de", "pt-BR", "ja", "hu"]
    private let panelWidth: CGFloat = 360

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

    private func maxLineWidth(for text: String, font: NSFont) -> CGFloat {
        text
            .components(separatedBy: .newlines)
            .map { width(for: $0, font: font) }
            .max() ?? 0
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
            "app.settings.label.language.hungarian",
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

    func testOnboardingStringsFitPanelLayoutsAcrossLocales() throws {
        let titleFont = NSFont.systemFont(ofSize: 17, weight: .semibold)
        let subtitleFont = NSFont.systemFont(ofSize: 12)
        let buttonFont = NSFont.systemFont(ofSize: 13, weight: .semibold)

        let onboardingTitleMaxWidth = panelWidth - 36
        let statusLabelMaxWidth = panelWidth - 56
        let buttonLabelMaxWidth = panelWidth - 120 // horizontal padding 40 each side + button insets

        let titleKeys = [
            "app.onboarding.welcome.title",
            "app.onboarding.detection.title",
            "app.onboarding.configure.title",
        ]
        let statusKeys = [
            ("app.onboarding.detection.scanning", onboardingTitleMaxWidth),
            ("app.onboarding.detection.none_detected", onboardingTitleMaxWidth),
            ("app.onboarding.configure.none_detected", statusLabelMaxWidth),
        ]
        let buttonKeys = [
            "app.onboarding.welcome.action.get_started",
            "app.onboarding.detection.action.continue",
            "app.onboarding.configure.action.finish_setup",
        ]

        for locale in locales {
            let strings = try localeAppStrings(locale)

            for key in titleKeys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    maxLineWidth(for: text, font: titleFont),
                    onboardingTitleMaxWidth,
                    "Onboarding title overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }

            for (key, maxWidth) in statusKeys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    maxLineWidth(for: text, font: subtitleFont),
                    maxWidth,
                    "Onboarding status-label overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }

            for key in buttonKeys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    maxLineWidth(for: text, font: buttonFont),
                    buttonLabelMaxWidth,
                    "Onboarding button overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }
        }
    }

    func testNavigationAndFilterStringsFitPanelLayoutsAcrossLocales() throws {
        let panelContentWidth = panelWidth - 24 // outer horizontal padding 12 each side
        let navTabFont = NSFont.systemFont(ofSize: 13, weight: .semibold)
        let searchFont = NSFont.systemFont(ofSize: 12)
        let filterFont = NSFont.systemFont(ofSize: 12)
        let managerMenuFont = NSFont.systemFont(ofSize: 11)

        let navTabsBudget = panelContentWidth - 20 // reserve room for settings button
        let searchFieldTextBudget = panelContentWidth - 46 // search icon + clear affordance + field padding
        let packageFilterBudget = panelContentWidth - 36 // reserve room for manager menu and spacing
        let managerMenuTextBudget: CGFloat = 130

        let tabKeys = [
            "app.navigation.tab.dashboard",
            "app.navigation.tab.packages",
            "app.navigation.tab.managers",
        ]
        let filterKeys = [
            "app.packages.filter.installed",
            "app.packages.filter.upgradable",
            "app.packages.filter.available",
        ]

        for locale in locales {
            let strings = try localeAppStrings(locale)

            let tabWidths = tabKeys.compactMap { strings[$0].map { width(for: $0, font: navTabFont) + 24 } }
            XCTAssertEqual(tabWidths.count, tabKeys.count, "Missing navigation key(s) in locale \(locale)")
            XCTAssertLessThanOrEqual(
                tabWidths.reduce(0, +),
                navTabsBudget,
                "Navigation tab overflow risk for locale \(locale)"
            )

            guard let searchPlaceholder = strings["app.navigation.search.placeholder"] else {
                XCTFail("Missing key app.navigation.search.placeholder in locale \(locale)")
                continue
            }
            XCTAssertLessThanOrEqual(
                maxLineWidth(for: searchPlaceholder, font: searchFont),
                searchFieldTextBudget,
                "Search placeholder overflow risk for locale \(locale): \(searchPlaceholder)"
            )

            let filterWidths = filterKeys.compactMap { strings[$0].map { width(for: $0, font: filterFont) + 16 } }
            XCTAssertEqual(filterWidths.count, filterKeys.count, "Missing package filter key(s) in locale \(locale)")
            XCTAssertLessThanOrEqual(
                filterWidths.reduce(0, +) + 8, // HStack spacing between 3 buttons
                packageFilterBudget,
                "Package filter button overflow risk for locale \(locale)"
            )

            guard let allManagers = strings["app.packages.filter.all_managers"] else {
                XCTFail("Missing key app.packages.filter.all_managers in locale \(locale)")
                continue
            }
            XCTAssertLessThanOrEqual(
                maxLineWidth(for: allManagers, font: managerMenuFont),
                managerMenuTextBudget,
                "Manager filter menu label overflow risk for locale \(locale): \(allManagers)"
            )
        }
    }

    func testManagerSectionLabelsFitPanelLayoutsAcrossLocales() throws {
        let panelContentWidth = panelWidth - 24 // row horizontal padding 12 each side
        let categoryFont = NSFont.systemFont(ofSize: 12, weight: .semibold)
        let stateFont = NSFont.systemFont(ofSize: 11)
        let categoryMaxWidth = panelContentWidth
        let stateMaxWidth: CGFloat = 96

        let categoryKeys = [
            "app.managers.category.toolchain",
            "app.managers.category.system_os",
            "app.managers.category.language",
            "app.managers.category.app_store",
        ]
        let stateKeys = [
            "app.managers.state.enabled",
            "app.managers.state.disabled",
            "app.managers.state.not_installed",
            "app.managers.state.coming_soon",
        ]

        for locale in locales {
            let strings = try localeAppStrings(locale)

            for key in categoryKeys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    maxLineWidth(for: text.uppercased(), font: categoryFont),
                    categoryMaxWidth,
                    "Managers category overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }

            for key in stateKeys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    maxLineWidth(for: text, font: stateFont),
                    stateMaxWidth,
                    "Managers state overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }

        }
    }
}
