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
    let sparkleFeedURL: String?
    let sparklePublicEdKey: String?

    var hasSparkleConfig: Bool {
        sparkleFeedURL != nil && sparklePublicEdKey != nil
    }

    var canUseSparkle: Bool {
        channel == .developerID && sparkleEnabled && hasSparkleConfig
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

        let sparkleFeedURL = (bundle.object(forInfoDictionaryKey: "SUFeedURL") as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let sparklePublicEdKey = (bundle.object(forInfoDictionaryKey: "SUPublicEDKey") as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines)

        return AppUpdateConfiguration(
            channel: channel,
            sparkleEnabled: sparkleEnabled,
            sparkleFeedURL: sparkleFeedURL?.isEmpty == true ? nil : sparkleFeedURL,
            sparklePublicEdKey: sparklePublicEdKey?.isEmpty == true ? nil : sparklePublicEdKey
        )
    }
}
