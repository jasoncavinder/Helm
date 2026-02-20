import Foundation

enum HelmDistributionChannel: String {
    case developerID = "developer_id"
    case appStore = "app_store"
    case setapp = "setapp"
    case fleet = "fleet"
    case unknown = "unknown"

    static func from(bundle: Bundle = .main) -> HelmDistributionChannel {
        guard let rawValue = bundle.object(forInfoDictionaryKey: "HelmDistributionChannel") as? String else {
            return .developerID
        }
        return HelmDistributionChannel(rawValue: rawValue) ?? .unknown
    }
}

struct AppUpdateConfiguration {
    let channel: HelmDistributionChannel
    let sparkleEnabled: Bool
    let sparkleAllowsDowngrades: Bool
    let sparkleFeedURL: String?
    let sparklePublicEdKey: String?
    let bundlePath: String

    var hasSparkleConfig: Bool {
        sparkleFeedURL != nil && sparklePublicEdKey != nil
    }

    var appearsMountedFromDiskImage: Bool {
        bundlePath.hasPrefix("/Volumes/")
    }

    var appearsTranslocated: Bool {
        bundlePath.contains("/AppTranslocation/")
    }

    var hasEligibleInstallLocation: Bool {
        !appearsMountedFromDiskImage && !appearsTranslocated
    }

    var hasSecureSparkleFeedURL: Bool {
        guard
            let sparkleFeedURL,
            let url = URL(string: sparkleFeedURL),
            url.scheme?.lowercased() == "https",
            url.host != nil
        else {
            return false
        }
        return true
    }

    var canUseSparkle: Bool {
        channel == .developerID &&
            sparkleEnabled &&
            !sparkleAllowsDowngrades &&
            hasSparkleConfig &&
            hasSecureSparkleFeedURL &&
            hasEligibleInstallLocation
    }

    static func from(bundle: Bundle = .main) -> AppUpdateConfiguration {
        let channel = HelmDistributionChannel.from(bundle: bundle)
        let sparkleEnabled: Bool
        if let boolValue = bundle.object(forInfoDictionaryKey: "HelmSparkleEnabled") as? Bool {
            sparkleEnabled = boolValue
        } else if let stringValue = bundle.object(forInfoDictionaryKey: "HelmSparkleEnabled") as? String {
            sparkleEnabled = ["1", "true", "yes"].contains(stringValue.lowercased())
        } else {
            sparkleEnabled = false
        }

        let sparkleAllowsDowngrades: Bool
        if let boolValue = bundle.object(forInfoDictionaryKey: "SUAllowsDowngrades") as? Bool {
            sparkleAllowsDowngrades = boolValue
        } else if let stringValue = bundle.object(forInfoDictionaryKey: "SUAllowsDowngrades") as? String {
            sparkleAllowsDowngrades = ["1", "true", "yes"].contains(stringValue.lowercased())
        } else {
            sparkleAllowsDowngrades = false
        }

        let sparkleFeedURL = (bundle.object(forInfoDictionaryKey: "SUFeedURL") as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let sparklePublicEdKey = (bundle.object(forInfoDictionaryKey: "SUPublicEDKey") as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines)

        return AppUpdateConfiguration(
            channel: channel,
            sparkleEnabled: sparkleEnabled,
            sparkleAllowsDowngrades: sparkleAllowsDowngrades,
            sparkleFeedURL: sparkleFeedURL?.isEmpty == true ? nil : sparkleFeedURL,
            sparklePublicEdKey: sparklePublicEdKey?.isEmpty == true ? nil : sparklePublicEdKey,
            bundlePath: bundle.bundleURL.path
        )
    }
}
