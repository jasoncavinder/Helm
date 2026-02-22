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

enum AppUpdateEligibilityFailure: String {
    case channelNotSupported = "channel_not_supported"
    case sparkleDisabled = "sparkle_disabled"
    case downgradesEnabled = "downgrades_enabled"
    case missingSparkleConfig = "missing_sparkle_config"
    case insecureSparkleFeed = "insecure_sparkle_feed"
    case ineligibleInstallLocation = "ineligible_install_location"
    case packageManagerManagedInstall = "package_manager_managed_install"
}

struct AppUpdateConfiguration {
    private static let defaultPackageManagerReceiptRoots = [
        "/opt/homebrew/Caskroom",
        "/usr/local/Caskroom"
    ]

    private static let packageManagerManagedPathPrefixes = [
        "/opt/homebrew/Caskroom/",
        "/usr/local/Caskroom/",
        "/opt/homebrew/Cellar/",
        "/usr/local/Cellar/",
        "/opt/local/",
        "/Applications/MacPorts/"
    ]

    let channel: HelmDistributionChannel
    let sparkleEnabled: Bool
    let sparkleAllowsDowngrades: Bool
    let sparkleFeedURL: String?
    let sparklePublicEdKey: String?
    let bundlePath: String
    let packageManagerReceiptRoots: [String]

    init(
        channel: HelmDistributionChannel,
        sparkleEnabled: Bool,
        sparkleAllowsDowngrades: Bool,
        sparkleFeedURL: String?,
        sparklePublicEdKey: String?,
        bundlePath: String,
        packageManagerReceiptRoots: [String] = []
    ) {
        self.channel = channel
        self.sparkleEnabled = sparkleEnabled
        self.sparkleAllowsDowngrades = sparkleAllowsDowngrades
        self.sparkleFeedURL = sparkleFeedURL
        self.sparklePublicEdKey = sparklePublicEdKey
        self.bundlePath = bundlePath
        self.packageManagerReceiptRoots = packageManagerReceiptRoots
    }

    private var resolvedBundlePath: String {
        URL(fileURLWithPath: bundlePath).resolvingSymlinksInPath().path
    }

    private var installPathCandidates: [String] {
        if resolvedBundlePath == bundlePath {
            return [bundlePath]
        }
        return [bundlePath, resolvedBundlePath]
    }

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

    var appearsPackageManagerManaged: Bool {
        let pathMatches = installPathCandidates.contains { candidatePath in
            Self.packageManagerManagedPathPrefixes.contains { candidatePath.hasPrefix($0) }
        }
        return pathMatches || hasPackageManagerReceiptForCurrentApp
    }

    private var hasPackageManagerReceiptForCurrentApp: Bool {
        let appName = URL(fileURLWithPath: bundlePath).lastPathComponent
        guard !appName.isEmpty else {
            return false
        }
        let fileManager = FileManager.default

        for root in packageManagerReceiptRoots {
            var rootIsDirectory: ObjCBool = false
            guard fileManager.fileExists(atPath: root, isDirectory: &rootIsDirectory), rootIsDirectory.boolValue else {
                continue
            }

            guard let tokenDirs = try? fileManager.contentsOfDirectory(atPath: root) else {
                continue
            }

            for tokenDir in tokenDirs {
                let tokenPath = (root as NSString).appendingPathComponent(tokenDir)
                var tokenIsDirectory: ObjCBool = false
                guard fileManager.fileExists(atPath: tokenPath, isDirectory: &tokenIsDirectory), tokenIsDirectory.boolValue else {
                    continue
                }

                guard let versionDirs = try? fileManager.contentsOfDirectory(atPath: tokenPath) else {
                    continue
                }

                for versionDir in versionDirs {
                    let appPath = ((tokenPath as NSString).appendingPathComponent(versionDir) as NSString)
                        .appendingPathComponent(appName)
                    var appIsDirectory: ObjCBool = false
                    if fileManager.fileExists(atPath: appPath, isDirectory: &appIsDirectory), appIsDirectory.boolValue {
                        return true
                    }
                }
            }
        }

        return false
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
        eligibilityFailureReason == nil
    }

    var eligibilityFailureReason: AppUpdateEligibilityFailure? {
        guard channel == .developerID else {
            return .channelNotSupported
        }
        guard sparkleEnabled else {
            return .sparkleDisabled
        }
        guard !sparkleAllowsDowngrades else {
            return .downgradesEnabled
        }
        guard hasSparkleConfig else {
            return .missingSparkleConfig
        }
        guard hasSecureSparkleFeedURL else {
            return .insecureSparkleFeed
        }
        guard hasEligibleInstallLocation else {
            return .ineligibleInstallLocation
        }
        guard !appearsPackageManagerManaged else {
            return .packageManagerManagedInstall
        }
        return nil
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
            bundlePath: bundle.bundleURL.path,
            packageManagerReceiptRoots: Self.defaultPackageManagerReceiptRoots
        )
    }
}
