import Foundation
import AppKit

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
    case bundleVersionMetadataMismatch = "bundle_version_metadata_mismatch"
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
    let bundleShortVersion: String?
    let bundleVersion: String?
    let bundlePath: String
    let packageManagerReceiptRoots: [String]

    init(
        channel: HelmDistributionChannel,
        sparkleEnabled: Bool,
        sparkleAllowsDowngrades: Bool,
        sparkleFeedURL: String?,
        sparklePublicEdKey: String?,
        bundleShortVersion: String? = nil,
        bundleVersion: String? = nil,
        bundlePath: String,
        packageManagerReceiptRoots: [String] = []
    ) {
        self.channel = channel
        self.sparkleEnabled = sparkleEnabled
        self.sparkleAllowsDowngrades = sparkleAllowsDowngrades
        self.sparkleFeedURL = sparkleFeedURL
        self.sparklePublicEdKey = sparklePublicEdKey
        self.bundleShortVersion = bundleShortVersion
        self.bundleVersion = bundleVersion
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

    private var hasConsistentBundleVersionMetadata: Bool {
        guard channel == .developerID, sparkleEnabled else {
            return true
        }
        guard
            let shortVersion = bundleShortVersion?.trimmingCharacters(in: .whitespacesAndNewlines),
            !shortVersion.isEmpty,
            let buildVersionRaw = bundleVersion?.trimmingCharacters(in: .whitespacesAndNewlines),
            let buildVersionNumber = Int64(buildVersionRaw)
        else {
            return true
        }

        let sparkleBuildSuffix = buildVersionNumber % 1000
        let shortVersionIsPrerelease = shortVersion.contains("-")
        if sparkleBuildSuffix < 900 {
            return shortVersionIsPrerelease
        }
        return !shortVersionIsPrerelease
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
        guard hasConsistentBundleVersionMetadata else {
            return .bundleVersionMetadataMismatch
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
        let bundleShortVersion = (bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let bundleVersion = (bundle.object(forInfoDictionaryKey: "CFBundleVersion") as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines)

        return AppUpdateConfiguration(
            channel: channel,
            sparkleEnabled: sparkleEnabled,
            sparkleAllowsDowngrades: sparkleAllowsDowngrades,
            sparkleFeedURL: sparkleFeedURL?.isEmpty == true ? nil : sparkleFeedURL,
            sparklePublicEdKey: sparklePublicEdKey?.isEmpty == true ? nil : sparklePublicEdKey,
            bundleShortVersion: bundleShortVersion?.isEmpty == true ? nil : bundleShortVersion,
            bundleVersion: bundleVersion?.isEmpty == true ? nil : bundleVersion,
            bundlePath: bundle.bundleURL.path,
            packageManagerReceiptRoots: Self.defaultPackageManagerReceiptRoots
        )
    }
}

enum PackageDescriptionRenderer {
    enum RenderedDescription {
        case plain(String)
        case rich(NSAttributedString)
    }

    static func render(_ summary: String?) -> RenderedDescription? {
        guard let summary = summary?.trimmingCharacters(in: .whitespacesAndNewlines),
              !summary.isEmpty else {
            return nil
        }
        if looksLikeHTML(summary) {
            if let attributed = htmlToAttributedText(summary) {
                return .rich(attributed)
            }
            if let normalized = htmlToReadableText(summary), !normalized.isEmpty {
                return .plain(normalized)
            }
        }
        return .plain(summary)
    }

    static func looksLikeHTML(_ text: String) -> Bool {
        let hasTag = text.range(
            of: #"<[a-zA-Z][\s\S]*?>"#,
            options: .regularExpression
        ) != nil
        let hasEntity = text.range(
            of: #"&[a-zA-Z0-9#]+;"#,
            options: .regularExpression
        ) != nil
        return hasTag || hasEntity
    }

    static func htmlToAttributedText(_ html: String) -> NSAttributedString? {
        guard let data = html.data(using: .utf8) else { return nil }
        let options: [NSAttributedString.DocumentReadingOptionKey: Any] = [
            .documentType: NSAttributedString.DocumentType.html,
            .characterEncoding: String.Encoding.utf8.rawValue
        ]
        guard let attributed = try? NSMutableAttributedString(
            data: data,
            options: options,
            documentAttributes: nil
        ) else {
            return nil
        }

        let rawText = attributed.string.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !rawText.isEmpty else { return nil }

        let fullRange = NSRange(location: 0, length: attributed.length)
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.lineBreakMode = .byWordWrapping
        paragraphStyle.paragraphSpacing = 4
        attributed.addAttribute(.paragraphStyle, value: paragraphStyle, range: fullRange)

        let fallbackFont = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
        attributed.enumerateAttribute(.font, in: fullRange) { value, range, _ in
            if value == nil {
                attributed.addAttribute(.font, value: fallbackFont, range: range)
            }
        }

        attributed.enumerateAttribute(.foregroundColor, in: fullRange) { value, range, _ in
            if value == nil {
                attributed.addAttribute(.foregroundColor, value: NSColor.labelColor, range: range)
            }
        }

        return attributed
    }

    static func htmlToReadableText(_ html: String) -> String? {
        guard let attributed = htmlToAttributedText(html) else { return nil }
        let nonBreakingSpaceNormalized = attributed.string.replacingOccurrences(of: "\u{00A0}", with: " ")
        let collapsedInlineWhitespace = nonBreakingSpaceNormalized.replacingOccurrences(
            of: #"[ \t\f\r]+"#,
            with: " ",
            options: .regularExpression
        )
        return collapsedInlineWhitespace
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
    }
}

enum InspectorLinkPolicy {
    private static let allowedSchemes = Set(["http", "https"])

    static func safeURL(from link: Any) -> URL? {
        let resolvedURL: URL?
        if let url = link as? URL {
            resolvedURL = url
        } else if let string = link as? String {
            resolvedURL = URL(string: string)
        } else {
            resolvedURL = nil
        }

        guard
            let url = resolvedURL,
            let scheme = url.scheme?.lowercased(),
            allowedSchemes.contains(scheme),
            url.host?.isEmpty == false
        else {
            return nil
        }
        return url
    }
}
