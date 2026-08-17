import XCTest

final class SettingsPaneTests: XCTestCase {
    private let locales = ["en", "es", "de", "fr", "pt-BR", "ja", "hu"]

    private var repoRootURL: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }

    func testPaneOrderAndIdentifiersRemainStable() {
        XCTAssertEqual(
            SettingsPane.allCases.map(\.rawValue),
            ["general", "updates", "sources", "cli", "support"]
        )
    }

    func testEveryPaneHasPresentationMetadata() {
        for pane in SettingsPane.allCases {
            XCTAssertFalse(pane.titleKey.isEmpty, "Missing title key for \(pane.rawValue)")
            XCTAssertFalse(pane.icon.isEmpty, "Missing icon for \(pane.rawValue)")
        }
    }

    func testEveryPaneTitleKeyResolvesAcrossLocales() throws {
        for locale in locales {
            let catalogURL = repoRootURL
                .appendingPathComponent("locales")
                .appendingPathComponent(locale)
                .appendingPathComponent("app.json")
            let data = try Data(contentsOf: catalogURL)
            let strings = try JSONDecoder().decode([String: String].self, from: data)

            for pane in SettingsPane.allCases {
                XCTAssertNotNil(
                    strings[pane.titleKey],
                    "Missing Settings pane title key \(pane.titleKey) in locale \(locale)"
                )
            }
        }
    }

    func testWayfinderPopoverKeysResolveAcrossLocales() throws {
        let keys = [
            "app.popover.wayfinder.utilities",
            "app.popover.wayfinder.no_action_needed",
            "app.popover.wayfinder.route.hint",
            "app.popover.wayfinder.route.state.cached",
            "app.popover.wayfinder.context.environment_covered",
            "app.popover.wayfinder.context.plan_ready",
            "app.popover.wayfinder.context.updates_in_plan",
            "app.popover.wayfinder.context.work_in_progress",
            "app.popover.wayfinder.context.decision_required",
            "app.popover.wayfinder.context.recovery_available",
            "app.popover.wayfinder.context.environment_needs_review",
            "app.popover.wayfinder.context.manager_needs_decision",
            "app.popover.wayfinder.context.manager_needs_review",
            "app.popover.wayfinder.context.package_state_needs_review",
            "app.popover.wayfinder.context.local_views_available",
            "app.popover.wayfinder.context.local_views_detail",
            "app.popover.wayfinder.context.checking_environment",
            "app.popover.wayfinder.context.last_known_state",
            "app.popover.wayfinder.action.view_saved_state",
            "app.popover.wayfinder.action.review_recovery",
            "app.popover.wayfinder.check_again",
            "app.popover.wayfinder.check_when_online",
            "app.popover.wayfinder.freshness.working_now",
            "app.popover.wayfinder.freshness.not_checked",
            "app.popover.wayfinder.freshness.saved",
            "app.popover.wayfinder.freshness.checked",
            "app.popover.wayfinder.ready",
            "app.inspector.multi_instance.attention_title",
        ] + WayfinderPopoverRouteStage.allCases.map(\.titleKey)

        for locale in locales {
            let catalogURL = repoRootURL
                .appendingPathComponent("locales")
                .appendingPathComponent(locale)
                .appendingPathComponent("app.json")
            let data = try Data(contentsOf: catalogURL)
            let strings = try JSONDecoder().decode([String: String].self, from: data)

            for key in keys {
                XCTAssertNotNil(strings[key], "Missing popover key \(key) in locale \(locale)")
            }
        }
    }

    func testWayfinderPopoverManagerPlaceholdersUseSupportedSyntaxAcrossLocales() throws {
        let keys = [
            "app.popover.wayfinder.context.manager_needs_decision",
            "app.popover.wayfinder.context.manager_needs_review",
        ]

        for locale in locales {
            let catalogURL = repoRootURL
                .appendingPathComponent("locales")
                .appendingPathComponent(locale)
                .appendingPathComponent("app.json")
            let data = try Data(contentsOf: catalogURL)
            let strings = try JSONDecoder().decode([String: String].self, from: data)

            for key in keys {
                let value = try XCTUnwrap(strings[key])
                XCTAssertTrue(value.contains("{manager}"), "Missing manager placeholder in \(key) for \(locale)")
                XCTAssertFalse(value.contains("%{manager}"), "Unsupported manager placeholder in \(key) for \(locale)")
            }
        }
    }

    func testWayfinderPopoverFindingDetailIsTranslatedAcrossLocales() throws {
        let key = "app.inspector.multi_instance.attention_title"
        let catalogs = try Dictionary(uniqueKeysWithValues: locales.map { locale in
            let catalogURL = repoRootURL
                .appendingPathComponent("locales")
                .appendingPathComponent(locale)
                .appendingPathComponent("app.json")
            let data = try Data(contentsOf: catalogURL)
            let strings = try JSONDecoder().decode([String: String].self, from: data)
            return (locale, strings)
        })
        let english = try XCTUnwrap(catalogs["en"]?[key])

        for locale in locales where locale != "en" {
            let localized = try XCTUnwrap(catalogs[locale]?[key])
            XCTAssertNotEqual(localized, english, "Untranslated popover finding detail in \(locale)")
        }
    }

    func testWayfinderPopoverUtilityMenuIsTranslatedAcrossLocales() throws {
        let appKeys = [
            "app.overlay.about.check_updates",
            "app.settings.section.support_feedback",
            "app.overlay.about.title",
            "app.settings.action.quit",
        ]
        let commonKeys = ["common.button.settings"]
        let englishApp = try catalog(locale: "en", file: "app.json")
        let englishCommon = try catalog(locale: "en", file: "common.json")

        for locale in locales where locale != "en" {
            let localizedApp = try catalog(locale: locale, file: "app.json")
            let localizedCommon = try catalog(locale: locale, file: "common.json")

            for key in appKeys {
                let englishValue = try XCTUnwrap(englishApp[key])
                let localizedValue = try XCTUnwrap(localizedApp[key])
                XCTAssertNotEqual(
                    localizedValue,
                    englishValue,
                    "Untranslated popover utility-menu key \(key) in \(locale)"
                )
            }
            for key in commonKeys {
                let englishValue = try XCTUnwrap(englishCommon[key])
                let localizedValue = try XCTUnwrap(localizedCommon[key])
                XCTAssertNotEqual(
                    localizedValue,
                    englishValue,
                    "Untranslated popover utility-menu key \(key) in \(locale)"
                )
            }
        }
    }

    private func catalog(locale: String, file: String) throws -> [String: String] {
        let catalogURL = repoRootURL
            .appendingPathComponent("locales")
            .appendingPathComponent(locale)
            .appendingPathComponent(file)
        let data = try Data(contentsOf: catalogURL)
        return try JSONDecoder().decode([String: String].self, from: data)
    }
}
