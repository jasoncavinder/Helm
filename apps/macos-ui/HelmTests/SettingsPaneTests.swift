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
}
