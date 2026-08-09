import XCTest

final class SettingsPaneTests: XCTestCase {
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
}
