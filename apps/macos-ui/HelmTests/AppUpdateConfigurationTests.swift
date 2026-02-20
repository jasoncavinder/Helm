import XCTest

final class AppUpdateConfigurationTests: XCTestCase {
    func testCanUseSparkleRequiresDeveloperIdSecureFeedAndKey() {
        let fullyConfigured = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleAllowsDowngrades: false,
            sparkleFeedURL: "https://updates.example.com/appcast.xml",
            sparklePublicEdKey: "abc123",
            bundlePath: "/Applications/Helm.app"
        )
        XCTAssertTrue(fullyConfigured.canUseSparkle)
        XCTAssertNil(fullyConfigured.eligibilityFailureReason)

        let missingFeed = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleAllowsDowngrades: false,
            sparkleFeedURL: nil,
            sparklePublicEdKey: "abc123",
            bundlePath: "/Applications/Helm.app"
        )
        XCTAssertFalse(missingFeed.canUseSparkle)
        XCTAssertEqual(missingFeed.eligibilityFailureReason, .missingSparkleConfig)

        let appStoreChannel = AppUpdateConfiguration(
            channel: .appStore,
            sparkleEnabled: true,
            sparkleAllowsDowngrades: false,
            sparkleFeedURL: "https://updates.example.com/appcast.xml",
            sparklePublicEdKey: "abc123",
            bundlePath: "/Applications/Helm.app"
        )
        XCTAssertFalse(appStoreChannel.canUseSparkle)
        XCTAssertEqual(appStoreChannel.eligibilityFailureReason, .channelNotSupported)

        let insecureFeed = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleAllowsDowngrades: false,
            sparkleFeedURL: "http://updates.example.com/appcast.xml",
            sparklePublicEdKey: "abc123",
            bundlePath: "/Applications/Helm.app"
        )
        XCTAssertFalse(insecureFeed.canUseSparkle)
        XCTAssertEqual(insecureFeed.eligibilityFailureReason, .insecureSparkleFeed)

        let malformedFeed = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleAllowsDowngrades: false,
            sparkleFeedURL: "not a url",
            sparklePublicEdKey: "abc123",
            bundlePath: "/Applications/Helm.app"
        )
        XCTAssertFalse(malformedFeed.canUseSparkle)
        XCTAssertEqual(malformedFeed.eligibilityFailureReason, .insecureSparkleFeed)

        let allowsDowngrades = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleAllowsDowngrades: true,
            sparkleFeedURL: "https://updates.example.com/appcast.xml",
            sparklePublicEdKey: "abc123",
            bundlePath: "/Applications/Helm.app"
        )
        XCTAssertFalse(allowsDowngrades.canUseSparkle)
        XCTAssertEqual(allowsDowngrades.eligibilityFailureReason, .downgradesEnabled)

        let mountedFromDMG = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleAllowsDowngrades: false,
            sparkleFeedURL: "https://updates.example.com/appcast.xml",
            sparklePublicEdKey: "abc123",
            bundlePath: "/Volumes/Helm/Helm.app"
        )
        XCTAssertFalse(mountedFromDMG.canUseSparkle)
        XCTAssertEqual(mountedFromDMG.eligibilityFailureReason, .ineligibleInstallLocation)

        let translocated = AppUpdateConfiguration(
            channel: .developerID,
            sparkleEnabled: true,
            sparkleAllowsDowngrades: false,
            sparkleFeedURL: "https://updates.example.com/appcast.xml",
            sparklePublicEdKey: "abc123",
            bundlePath: "/private/var/folders/tmp/AppTranslocation/ABC123/Helm.app"
        )
        XCTAssertFalse(translocated.canUseSparkle)
        XCTAssertEqual(translocated.eligibilityFailureReason, .ineligibleInstallLocation)
    }

    func testFromBundleParsesChannelAndSparkleSettings() throws {
        let bundle = try makeBundle(info: [
            "HelmDistributionChannel": "fleet",
            "HelmSparkleEnabled": "YES",
            "SUAllowsDowngrades": "NO",
            "SUFeedURL": " https://updates.example.com/appcast.xml ",
            "SUPublicEDKey": " test-key "
        ])
        defer { removeBundle(bundle) }

        let config = AppUpdateConfiguration.from(bundle: bundle)
        XCTAssertEqual(config.channel, .fleet)
        XCTAssertTrue(config.sparkleEnabled)
        XCTAssertFalse(config.sparkleAllowsDowngrades)
        XCTAssertEqual(config.sparkleFeedURL, "https://updates.example.com/appcast.xml")
        XCTAssertEqual(config.sparklePublicEdKey, "test-key")
        XCTAssertEqual(config.bundlePath, bundle.bundleURL.path)
        XCTAssertFalse(config.canUseSparkle)
        XCTAssertEqual(config.eligibilityFailureReason, .channelNotSupported)
    }

    func testFromBundleDefaultsToDeveloperIdWhenChannelMissing() throws {
        let bundle = try makeBundle(info: [:])
        defer { removeBundle(bundle) }

        let config = AppUpdateConfiguration.from(bundle: bundle)
        XCTAssertEqual(config.channel, .developerID)
        XCTAssertFalse(config.sparkleEnabled)
        XCTAssertFalse(config.sparkleAllowsDowngrades)
        XCTAssertNil(config.sparkleFeedURL)
        XCTAssertNil(config.sparklePublicEdKey)
        XCTAssertEqual(config.bundlePath, bundle.bundleURL.path)
        XCTAssertEqual(config.eligibilityFailureReason, .sparkleDisabled)
    }

    func testFromBundleTreatsBlankSparkleFeedAndKeyAsMissing() throws {
        let bundle = try makeBundle(info: [
            "HelmDistributionChannel": "developer_id",
            "HelmSparkleEnabled": "YES",
            "SUAllowsDowngrades": "YES",
            "SUFeedURL": "   ",
            "SUPublicEDKey": ""
        ])
        defer { removeBundle(bundle) }

        let config = AppUpdateConfiguration.from(bundle: bundle)
        XCTAssertTrue(config.sparkleAllowsDowngrades)
        XCTAssertNil(config.sparkleFeedURL)
        XCTAssertNil(config.sparklePublicEdKey)
        XCTAssertFalse(config.hasSparkleConfig)
        XCTAssertFalse(config.canUseSparkle)
        XCTAssertEqual(config.eligibilityFailureReason, .downgradesEnabled)
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
