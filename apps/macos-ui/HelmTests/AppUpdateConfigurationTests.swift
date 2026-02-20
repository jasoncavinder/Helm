import XCTest

final class AppUpdateConfigurationTests: XCTestCase {
    func testCanUseSparkleRequiresDeveloperIdSecureFeedAndKey() {
        let fullyConfigured = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleFeedURL: "https://updates.example.com/appcast.xml",
            sparklePublicEdKey: "abc123"
        )
        XCTAssertTrue(fullyConfigured.canUseSparkle)

        let missingFeed = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleFeedURL: nil,
            sparklePublicEdKey: "abc123"
        )
        XCTAssertFalse(missingFeed.canUseSparkle)

        let appStoreChannel = AppUpdateConfiguration(
            channel: .appStore,
            sparkleEnabled: true,
            sparkleFeedURL: "https://updates.example.com/appcast.xml",
            sparklePublicEdKey: "abc123"
        )
        XCTAssertFalse(appStoreChannel.canUseSparkle)

        let insecureFeed = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleFeedURL: "http://updates.example.com/appcast.xml",
            sparklePublicEdKey: "abc123"
        )
        XCTAssertFalse(insecureFeed.canUseSparkle)

        let malformedFeed = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleFeedURL: "not a url",
            sparklePublicEdKey: "abc123"
        )
        XCTAssertFalse(malformedFeed.canUseSparkle)
    }

    func testFromBundleParsesChannelAndSparkleSettings() throws {
        let bundle = try makeBundle(info: [
            "HelmDistributionChannel": "fleet",
            "HelmSparkleEnabled": "YES",
            "SUFeedURL": " https://updates.example.com/appcast.xml ",
            "SUPublicEDKey": " test-key "
        ])
        defer { removeBundle(bundle) }

        let config = AppUpdateConfiguration.from(bundle: bundle)
        XCTAssertEqual(config.channel, .fleet)
        XCTAssertTrue(config.sparkleEnabled)
        XCTAssertEqual(config.sparkleFeedURL, "https://updates.example.com/appcast.xml")
        XCTAssertEqual(config.sparklePublicEdKey, "test-key")
        XCTAssertFalse(config.canUseSparkle)
    }

    func testFromBundleDefaultsToDeveloperIdWhenChannelMissing() throws {
        let bundle = try makeBundle(info: [:])
        defer { removeBundle(bundle) }

        let config = AppUpdateConfiguration.from(bundle: bundle)
        XCTAssertEqual(config.channel, .developerID)
        XCTAssertFalse(config.sparkleEnabled)
        XCTAssertNil(config.sparkleFeedURL)
        XCTAssertNil(config.sparklePublicEdKey)
    }

    func testFromBundleTreatsBlankSparkleFeedAndKeyAsMissing() throws {
        let bundle = try makeBundle(info: [
            "HelmDistributionChannel": "developer_id",
            "HelmSparkleEnabled": "YES",
            "SUFeedURL": "   ",
            "SUPublicEDKey": ""
        ])
        defer { removeBundle(bundle) }

        let config = AppUpdateConfiguration.from(bundle: bundle)
        XCTAssertNil(config.sparkleFeedURL)
        XCTAssertNil(config.sparklePublicEdKey)
        XCTAssertFalse(config.hasSparkleConfig)
        XCTAssertFalse(config.canUseSparkle)
    }

    private func makeBundle(info: [String: Any]) throws -> Bundle {
        let bundleURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("helm-update-config-\(UUID().uuidString)")
            .appendingPathExtension("bundle")
        try FileManager.default.createDirectory(at: bundleURL, withIntermediateDirectories: true)

        let plistURL = bundleURL.appendingPathComponent("Info.plist")
        let plistData = try PropertyListSerialization.data(
            fromPropertyList: info,
            format: .xml,
            options: 0
        )
        try plistData.write(to: plistURL)

        guard let bundle = Bundle(url: bundleURL) else {
            XCTFail("Failed to create test bundle at \(bundleURL.path)")
            throw NSError(domain: "AppUpdateConfigurationTests", code: 1)
        }
        return bundle
    }

    private func removeBundle(_ bundle: Bundle) {
        guard let bundleURL = bundle.bundleURL as URL? else { return }
        try? FileManager.default.removeItem(at: bundleURL)
    }
}
