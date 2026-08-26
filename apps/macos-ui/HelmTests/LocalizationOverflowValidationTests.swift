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

    func testEnvironmentBriefReadinessAnnouncementsLeadWithLocalizedTitles() throws {
        let allLocales = ["en"] + locales
        let templateKey = "app.first_run.environment_brief.readiness.accessibility_label"

        for locale in allLocales {
            let strings = try localeAppStrings(locale)
            let template = try XCTUnwrap(strings[templateKey])
            let titleRange = try XCTUnwrap(template.range(of: "{title}"))
            let countRange = try XCTUnwrap(template.range(of: "{count}"))

            XCTAssertLessThan(
                titleRange.lowerBound,
                countRange.lowerBound,
                "Environment Brief readiness announcement order drifted in \(locale)"
            )
        }

        let english = try localeAppStrings("en")
        XCTAssertEqual(
            english["app.first_run.environment_brief.readiness.attention"],
            "Needs review"
        )
    }

    func testEnvironmentBriefCourseIndicatorAnnouncementsUseLocalizedPercentagesAndLeadWithTitles() throws {
        let allLocales = ["en"] + locales
        let templateKey = "app.first_run.environment_brief.course_indicator.accessibility_label"
        let titleKey = "app.first_run.environment_brief.section.sources"
        let expectedPercentages = [
            "en": "50%",
            "es": "50\u{00a0}%",
            "fr": "50\u{00a0}%",
            "de": "50\u{00a0}%",
            "pt-BR": "50%",
            "ja": "50%",
            "hu": "50%",
        ]

        for locale in allLocales {
            let strings = try localeAppStrings(locale)
            let template = try XCTUnwrap(strings[templateKey])
            let title = try XCTUnwrap(strings[titleKey])
            let expectedPercentage = try XCTUnwrap(expectedPercentages[locale])
            let titleRange = try XCTUnwrap(template.range(of: "{title}"))
            let percentageRange = try XCTUnwrap(template.range(of: "{percentage}"))

            XCTAssertLessThan(
                titleRange.lowerBound,
                percentageRange.lowerBound,
                "Environment Brief Course Indicator announcement order drifted in \(locale)"
            )

            let label = EnvironmentBriefCourseLabelFormatter.label(
                template: template,
                title: title,
                fraction: 0.5,
                localeIdentifier: locale
            )
            let expectedLabel = template
                .replacingOccurrences(of: "{title}", with: title)
                .replacingOccurrences(of: "{percentage}", with: expectedPercentage)

            XCTAssertEqual(
                label,
                expectedLabel,
                "Environment Brief Course Indicator formatting drifted in \(locale)"
            )
        }
    }

    func testTaskFiveHungarianVisibleSurfaceStringsAreLocalized() throws {
        let english = try localeAppStrings("en")
        let hungarian = try localeAppStrings("hu")
        let keys = [
            "app.navigation.tab.dashboard",
            "app.settings.tab.title",
            "app.health.healthy",
            "app.health.error",
            "app.health.running",
            "app.health.not_installed",
            "app.overview.manager_health",
            "app.overview.recent_tasks",
            "app.packages.filter.upgradable",
            "app.settings.section.service_health",
            "app.settings.service_health.connection",
            "app.settings.service_health.refresh_state",
            "app.settings.service_health.last_check",
            "app.settings.service_health.failed_tasks",
            "app.settings.service_health.last_error",
            "app.settings.service_health.copy_snapshot",
            "app.settings.service_health.status.connected",
            "app.settings.service_health.status.disconnected",
            "app.settings.service_health.status.refreshing",
            "app.settings.service_health.status.idle",
            "app.settings.service_health.status.never",
            "app.control_center.search_placeholder",
            "app.inspector.title",
            "app.inspector.empty",
            "app.managers.research.instance.inactive",
        ]

        for key in keys {
            let englishValue = try XCTUnwrap(english[key], "Missing English value for \(key)")
            let hungarianValue = try XCTUnwrap(hungarian[key], "Missing Hungarian value for \(key)")
            XCTAssertNotEqual(
                hungarianValue,
                englishValue,
                "Task 5 Hungarian presentation must not reuse English for \(key)"
            )
        }
    }

    func testHungarianRefreshingStatusCommunicatesInProgressState() throws {
        let hungarian = try localeAppStrings("hu")

        XCTAssertEqual(
            hungarian["app.settings.service_health.status.refreshing"],
            "Frissítés folyamatban"
        )
    }

    func testLanguagePickerOptionsFitConfiguredWidthAcrossLocales() throws {
        let keys = [
            "app.settings.label.language.system_default",
            "app.settings.label.language.english",
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
        let packageFilterChipMaxWidth = panelContentWidth - 24
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
            "app.packages.filter.pinned",
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
            let maxFilterChipWidth = filterWidths.max() ?? 0
            XCTAssertLessThanOrEqual(
                maxFilterChipWidth,
                packageFilterChipMaxWidth,
                "Package filter chip overflow risk for locale \(locale)"
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

    func testEnvironmentBriefCompactLabelsFitMinimumLayoutAcrossLocales() throws {
        let actionFont = NSFont.systemFont(ofSize: 13, weight: .semibold)
        let statusFont = NSFont.systemFont(ofSize: 12, weight: .semibold)
        let trustFont = NSFont.systemFont(ofSize: 13)

        let actionMaxWidth: CGFloat = 180
        let statusMaxWidth: CGFloat = 180
        let trustStripMaxWidth: CGFloat = 780
        let trustLabelChrome: CGFloat = 22
        let trustSpacing: CGFloat = 36

        let actionKeys = [
            "app.first_run.environment_brief.action.use_helm",
            "app.first_run.environment_brief.action.scan_again",
        ]
        let statusKeys = [
            "app.first_run.environment_brief.status.ready",
            "app.first_run.environment_brief.status.cached",
            "app.first_run.environment_brief.status.setup_required",
            "app.first_run.environment_brief.status.multiple_instances",
            "app.first_run.environment_brief.status.protected",
            "app.first_run.environment_brief.status.reviewing",
            "app.first_run.environment_brief.status.failed",
            "app.first_run.environment_brief.status.cancelled",
            "app.first_run.environment_brief.status.deferred",
        ]
        let localTrustKeys = [
            "app.first_run.environment_brief.trust.local",
            "app.first_run.environment_brief.trust.no_changes",
            "app.first_run.environment_brief.trust.no_network",
        ]
        let networkTrustKeys = [
            "app.first_run.environment_brief.trust.local",
            "app.first_run.environment_brief.trust.no_changes",
            "app.first_run.environment_brief.trust.disclosed_network",
        ]

        for locale in locales {
            let strings = try localeAppStrings(locale)

            for key in actionKeys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    width(for: text, font: actionFont),
                    actionMaxWidth,
                    "Environment Brief action overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }

            for key in statusKeys {
                guard let text = strings[key] else {
                    XCTFail("Missing key \(key) in locale \(locale)")
                    continue
                }
                XCTAssertLessThanOrEqual(
                    width(for: text, font: statusFont),
                    statusMaxWidth,
                    "Environment Brief status overflow risk for locale \(locale): \(key) -> \(text)"
                )
            }

            for keys in [localTrustKeys, networkTrustKeys] {
                let labels = keys.compactMap { strings[$0] }
                XCTAssertEqual(labels.count, keys.count, "Missing trust label(s) in locale \(locale)")
                let renderedWidth = labels.reduce(0) {
                    $0 + width(for: $1, font: trustFont) + trustLabelChrome
                } + trustSpacing
                XCTAssertLessThanOrEqual(
                    renderedWidth,
                    trustStripMaxWidth,
                    "Environment Brief trust strip overflow risk for locale \(locale)"
                )
            }
        }
    }

    func testTaskSevenFirstRunCopyIsCompleteAndMirroredAcrossLocales() throws {
        let allLocales = ["en"] + locales
        let english = try localeAppStrings("en")
        let requiredKeys = Set(english.keys.filter {
            $0.hasPrefix("app.first_run.research.")
                || $0.hasPrefix("research.first_run.")
        })

        XCTAssertEqual(requiredKeys.count, 49)

        for locale in allLocales {
            let strings = try localeAppStrings(locale)
            let availableKeys = Set(strings.keys)
            XCTAssertTrue(
                requiredKeys.isSubset(of: availableKeys),
                "Missing Task 7 first-run copy in locale \(locale): \(requiredKeys.subtracting(availableKeys).sorted())"
            )
            for key in requiredKeys {
                let text = try XCTUnwrap(strings[key])
                XCTAssertFalse(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                XCTAssertNotEqual(text, key, "Locale \(locale) exposes a raw key for \(key)")
            }

            let canonicalURL = repoRootURL
                .appendingPathComponent("locales")
                .appendingPathComponent(locale)
                .appendingPathComponent("app.json")
            let bundledURL = repoRootURL
                .appendingPathComponent("apps/macos-ui/Helm/Resources/locales")
                .appendingPathComponent(locale)
                .appendingPathComponent("app.json")
            XCTAssertEqual(
                try Data(contentsOf: canonicalURL),
                try Data(contentsOf: bundledURL),
                "Canonical and bundled app catalogs drifted for locale \(locale)"
            )
        }
    }
}

final class LocalizationPreferenceTests: XCTestCase {
    private func makeDefaults() -> UserDefaults {
        let suiteName = "LocalizationPreferenceTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        return defaults
    }

    func testFreshInstallUsesSystemSelectionAndPreferredLanguage() {
        let defaults = makeDefaults()
        let store = LocalizationPreferenceStore(defaults: defaults)

        let selection = store.initialSelection()

        XCTAssertEqual(selection, LocalizationPreferenceStore.systemSelection)
        XCTAssertEqual(
            LocalizationPreferenceStore.effectiveLocale(
                for: selection,
                preferredLanguages: ["en-US"]
            ),
            "en-US"
        )
    }

    func testLegacyEnglishSelectionMigratesToSystemSelection() {
        let defaults = makeDefaults()
        defaults.set("en", forKey: LocalizationPreferenceStore.legacyPreferenceKey)
        let store = LocalizationPreferenceStore(defaults: defaults)

        XCTAssertEqual(store.initialSelection(), LocalizationPreferenceStore.systemSelection)
        XCTAssertNil(defaults.object(forKey: LocalizationPreferenceStore.legacyPreferenceKey))
        XCTAssertEqual(
            defaults.string(forKey: LocalizationPreferenceStore.preferenceKey),
            LocalizationPreferenceStore.systemSelection
        )
    }

    func testLegacyExplicitLanguageSelectionIsPreserved() {
        let defaults = makeDefaults()
        defaults.set("de", forKey: LocalizationPreferenceStore.legacyPreferenceKey)
        let store = LocalizationPreferenceStore(defaults: defaults)

        XCTAssertEqual(store.initialSelection(), "de")
        XCTAssertNil(defaults.object(forKey: LocalizationPreferenceStore.legacyPreferenceKey))
        XCTAssertEqual(
            defaults.string(forKey: LocalizationPreferenceStore.preferenceKey),
            "de"
        )
    }

    func testExplicitLanguagePersistsAcrossLaunches() {
        let defaults = makeDefaults()
        let store = LocalizationPreferenceStore(defaults: defaults)
        store.save("de")

        XCTAssertEqual(store.initialSelection(), "de")
        XCTAssertEqual(
            LocalizationPreferenceStore.effectiveLocale(
                for: store.initialSelection(),
                preferredLanguages: ["en-US"]
            ),
            "de"
        )
    }

    func testSystemSelectionTracksChangedPreferredLanguage() {
        XCTAssertEqual(
            LocalizationPreferenceStore.effectiveLocale(
                for: LocalizationPreferenceStore.systemSelection,
                preferredLanguages: ["de-DE"]
            ),
            "de-DE"
        )
        XCTAssertEqual(
            LocalizationPreferenceStore.effectiveLocale(
                for: LocalizationPreferenceStore.systemSelection,
                preferredLanguages: ["en-US"]
            ),
            "en-US"
        )
    }

    func testUnsupportedStoredSelectionFallsBackToSystem() {
        let defaults = makeDefaults()
        defaults.set("not-a-locale", forKey: LocalizationPreferenceStore.preferenceKey)

        XCTAssertEqual(
            LocalizationPreferenceStore(defaults: defaults).initialSelection(),
            LocalizationPreferenceStore.systemSelection
        )
    }
}
