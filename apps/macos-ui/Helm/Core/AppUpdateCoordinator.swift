import Combine
import Foundation
import os.log
#if canImport(Sparkle)
import Sparkle
#endif

private let updateLogger = Logger(subsystem: "com.jasoncavinder.Helm", category: "app_update")

struct AppUpdateAvailability: Equatable {
    let displayVersion: String
    let buildVersion: String
    let releaseNotesURL: URL?
    let channel: String?
    let isCritical: Bool
    let isMajorUpgrade: Bool
}

private enum AppUpdateCheckKind: Int {
    case userInitiated = 0
    case background = 1
    case information = 2
}

enum AppUpdateUnavailableReason: String {
    case channelNotSupported = "channel_not_supported"
    case sparkleDisabled = "sparkle_disabled"
    case downgradesEnabled = "downgrades_enabled"
    case missingSparkleConfig = "missing_sparkle_config"
    case insecureSparkleFeed = "insecure_sparkle_feed"
    case bundleVersionMetadataMismatch = "bundle_version_metadata_mismatch"
    case ineligibleInstallLocation = "ineligible_install_location"
    case packageManagerManagedInstall = "package_manager_managed_install"
    case sparkleFrameworkUnavailable = "sparkle_framework_unavailable"
    case sparkleRuntimeUnavailable = "sparkle_runtime_unavailable"

    var localizationKey: String {
        switch self {
        case .channelNotSupported:
            return L10n.App.Overlay.About.UpdateUnavailable.channelManaged
        case .ineligibleInstallLocation:
            return L10n.App.Overlay.About.UpdateUnavailable.installLocation
        case .packageManagerManagedInstall:
            return L10n.App.Overlay.About.UpdateUnavailable.packageManagerManaged
        case .sparkleFrameworkUnavailable:
            return L10n.App.Overlay.About.UpdateUnavailable.sparkleMissing
        case .sparkleRuntimeUnavailable:
            return L10n.App.Overlay.About.UpdateUnavailable.runtimeUnavailable
        case .sparkleDisabled, .downgradesEnabled, .missingSparkleConfig, .insecureSparkleFeed, .bundleVersionMetadataMismatch:
            return L10n.App.Overlay.About.UpdateUnavailable.buildConfig
        }
    }
}

private protocol AppUpdateDriver: AnyObject {
    var canCheckForUpdates: Bool { get }
    var automaticallyChecksForUpdates: Bool { get }
    var updateCheckInterval: TimeInterval { get }
    var lastUpdateCheckDate: Date? { get }
    var availabilityHandler: ((AppUpdateAvailability?) -> Void)? { get set }
    var checkCompletionHandler: ((AppUpdateCheckKind, Error?) -> Void)? { get set }
    func checkForUpdates()
    func checkForUpdateInformation()
    func setPrereleaseUpdatesEnabled(_ enabled: Bool)
}

private final class NoopAppUpdateDriver: AppUpdateDriver {
    let canCheckForUpdates = false
    let automaticallyChecksForUpdates = false
    let updateCheckInterval: TimeInterval = 86_400
    let lastUpdateCheckDate: Date? = nil
    var availabilityHandler: ((AppUpdateAvailability?) -> Void)?
    var checkCompletionHandler: ((AppUpdateCheckKind, Error?) -> Void)?

    func checkForUpdates() {}
    func checkForUpdateInformation() {}
    func setPrereleaseUpdatesEnabled(_ enabled: Bool) {}
}

#if canImport(Sparkle)
private final class SparkleAppUpdateChannelDelegate: NSObject, SPUUpdaterDelegate {
    var prereleaseUpdatesEnabled: Bool
    var availabilityHandler: ((AppUpdateAvailability?) -> Void)?
    var checkCompletionHandler: ((AppUpdateCheckKind, Error?) -> Void)?

    init(prereleaseUpdatesEnabled: Bool) {
        self.prereleaseUpdatesEnabled = prereleaseUpdatesEnabled
    }

    func allowedChannels(for updater: SPUUpdater) -> Set<String> {
        HelmSparkleUpdateChannel.allowedChannels(
            prereleaseUpdatesEnabled: prereleaseUpdatesEnabled
        )
    }

    func updater(_ updater: SPUUpdater, didFindValidUpdate item: SUAppcastItem) {
        availabilityHandler?(
            AppUpdateAvailability(
                displayVersion: item.displayVersionString,
                buildVersion: item.versionString,
                releaseNotesURL: item.releaseNotesURL,
                channel: item.channel,
                isCritical: item.isCriticalUpdate,
                isMajorUpgrade: item.isMajorUpgrade
            )
        )
    }

    func updaterDidNotFindUpdate(_ updater: SPUUpdater) {
        availabilityHandler?(nil)
    }

    func updater(
        _ updater: SPUUpdater,
        didFinishUpdateCycleFor updateCheck: SPUUpdateCheck,
        error: Error?
    ) {
        let checkKind = AppUpdateCheckKind(rawValue: updateCheck.rawValue) ?? .userInitiated
        checkCompletionHandler?(checkKind, error)
    }
}

private final class SparkleAppUpdateDriver: NSObject, AppUpdateDriver {
    private let channelDelegate: SparkleAppUpdateChannelDelegate
    private let updaterController: SPUStandardUpdaterController
    private let automaticChecksWereEnabled: Bool
    private let configuredUpdateCheckInterval: TimeInterval

    init(configuration: AppUpdateConfiguration, prereleaseUpdatesEnabled: Bool) {
        let channelDelegate = SparkleAppUpdateChannelDelegate(
            prereleaseUpdatesEnabled: prereleaseUpdatesEnabled
        )
        self.channelDelegate = channelDelegate
        updaterController = SPUStandardUpdaterController(
            startingUpdater: false,
            updaterDelegate: channelDelegate,
            userDriverDelegate: nil
        )
        automaticChecksWereEnabled = updaterController.updater.automaticallyChecksForUpdates
        configuredUpdateCheckInterval = updaterController.updater.updateCheckInterval
        super.init()

        // Helm owns scheduling so automatic checks can be projected into Helm's UI
        // without Sparkle presenting its standard update alert on discovery.
        updaterController.updater.automaticallyChecksForUpdates = false
        updaterController.startUpdater()

        if let clearedFeedURL = updaterController.updater.clearFeedURLFromUserDefaults() {
            updateLogger.notice(
                "Cleared persisted Sparkle feed URL override from user defaults: \(clearedFeedURL.absoluteString, privacy: .public)"
            )
        }

        let configuredFeedURL = configuration.sparkleFeedURL ?? "none"
        let resolvedFeedURL = updaterController.updater.feedURL?.absoluteString ?? "none"
        updateLogger.info(
            "Sparkle updater initialized. can_check=\(self.updaterController.updater.canCheckForUpdates, privacy: .public), configured_feed_url=\(configuredFeedURL, privacy: .public), resolved_feed_url=\(resolvedFeedURL, privacy: .public)"
        )
    }

    var canCheckForUpdates: Bool {
        updaterController.updater.canCheckForUpdates
    }

    var automaticallyChecksForUpdates: Bool {
        automaticChecksWereEnabled
    }

    var updateCheckInterval: TimeInterval {
        configuredUpdateCheckInterval
    }

    var lastUpdateCheckDate: Date? {
        updaterController.updater.lastUpdateCheckDate
    }

    var availabilityHandler: ((AppUpdateAvailability?) -> Void)? {
        get { channelDelegate.availabilityHandler }
        set { channelDelegate.availabilityHandler = newValue }
    }

    var checkCompletionHandler: ((AppUpdateCheckKind, Error?) -> Void)? {
        get { channelDelegate.checkCompletionHandler }
        set { channelDelegate.checkCompletionHandler = newValue }
    }

    func checkForUpdates() {
        let resolvedFeedURL = updaterController.updater.feedURL?.absoluteString ?? "none"
        updateLogger.info(
            "Dispatching Sparkle update check. can_check=\(self.updaterController.updater.canCheckForUpdates, privacy: .public), feed_url=\(resolvedFeedURL, privacy: .public)"
        )
        updaterController.checkForUpdates(nil)
    }

    func checkForUpdateInformation() {
        updaterController.updater.checkForUpdateInformation()
    }

    func setPrereleaseUpdatesEnabled(_ enabled: Bool) {
        channelDelegate.prereleaseUpdatesEnabled = enabled
        updaterController.updater.resetUpdateCycleAfterShortDelay()
    }
}
#endif

final class AppUpdateCoordinator: ObservableObject {
    static let shared = AppUpdateCoordinator()
    private static let autoCheckEnabledKey = "appUpdate.autoCheckEnabled"
    private static let checkFrequencyMinutesKey = "appUpdate.checkFrequencyMinutes"
    private static let lastCheckDateKey = "appUpdate.lastCheckDate"
    private static let prereleaseUpdatesEnabledKey = "appUpdate.prereleaseUpdatesEnabled"
    private static let includeHelmInUpgradeAllKey = "appUpdate.includeHelmInUpgradeAll"

    @Published private(set) var configuration: AppUpdateConfiguration
    @Published private(set) var updateAuthority: HelmUpdateAuthority
    @Published private(set) var canCheckForUpdates: Bool
    @Published private(set) var unavailableReason: AppUpdateUnavailableReason?
    @Published private(set) var isCheckingForUpdates = false
    @Published private(set) var lastCheckDate: Date?
    @Published private(set) var autoCheckEnabled: Bool
    @Published private(set) var checkFrequencyMinutes: Int
    @Published private(set) var prereleaseUpdatesEnabled: Bool
    @Published private(set) var includeHelmInUpgradeAll: Bool
    @Published private(set) var availableUpdate: AppUpdateAvailability?

    var distributionChannel: HelmDistributionChannel {
        configuration.channel
    }

    private let driver: AppUpdateDriver
    private var automaticCheckTimer: Timer?
    private var networkAvailable = false
    private var pendingCheckKind: AppUpdateCheckKind?

    private init() {
        let configuration = AppUpdateConfiguration.from()
        let prereleaseUpdatesEnabled = UserDefaults.standard.bool(
            forKey: Self.prereleaseUpdatesEnabledKey
        )
        self.configuration = configuration
        self.updateAuthority = configuration.updateAuthority
        self.prereleaseUpdatesEnabled = prereleaseUpdatesEnabled
        self.includeHelmInUpgradeAll = UserDefaults.standard.bool(
            forKey: Self.includeHelmInUpgradeAllKey
        )
        self.availableUpdate = nil
        let selection = AppUpdateCoordinator.makeDriver(
            for: configuration,
            prereleaseUpdatesEnabled: prereleaseUpdatesEnabled
        )
        self.driver = selection.driver
        if let timestamp = UserDefaults.standard.object(forKey: Self.lastCheckDateKey) as? NSNumber {
            self.lastCheckDate = Date(timeIntervalSince1970: timestamp.doubleValue)
        } else {
            self.lastCheckDate = selection.driver.lastUpdateCheckDate
        }
        if let storedAutoCheck = UserDefaults.standard.object(forKey: Self.autoCheckEnabledKey) as? NSNumber {
            self.autoCheckEnabled = storedAutoCheck.boolValue
        } else {
            let migratedAutoCheckEnabled = selection.driver.automaticallyChecksForUpdates
            self.autoCheckEnabled = migratedAutoCheckEnabled
            UserDefaults.standard.set(migratedAutoCheckEnabled, forKey: Self.autoCheckEnabledKey)
        }
        if let storedFrequency = UserDefaults.standard.object(
            forKey: Self.checkFrequencyMinutesKey
        ) as? NSNumber {
            self.checkFrequencyMinutes = Self.supportedFrequencyMinutes(
                for: TimeInterval(storedFrequency.intValue * 60)
            )
        } else {
            let migratedFrequencyMinutes = Self.supportedFrequencyMinutes(
                for: selection.driver.updateCheckInterval
            )
            self.checkFrequencyMinutes = migratedFrequencyMinutes
            UserDefaults.standard.set(
                migratedFrequencyMinutes,
                forKey: Self.checkFrequencyMinutesKey
            )
        }
        if selection.driver.canCheckForUpdates {
            self.canCheckForUpdates = true
            self.unavailableReason = nil
        } else {
            self.canCheckForUpdates = false
            self.unavailableReason = selection.unavailableReason ?? .sparkleRuntimeUnavailable
        }

        driver.availabilityHandler = { [weak self] availability in
            DispatchQueue.main.async {
                self?.availableUpdate = availability
            }
        }
        driver.checkCompletionHandler = { [weak self] checkKind, error in
            DispatchQueue.main.async {
                self?.finishCheck(checkKind: checkKind, error: error)
            }
        }
        scheduleAutomaticCheck()

        updateLogger.info(
            "Configured app updater. channel=\(configuration.channel.rawValue, privacy: .public), authority=\(self.updateAuthority.rawValue, privacy: .public), sparkle_enabled=\(configuration.sparkleEnabled, privacy: .public), sparkle_allows_downgrades=\(configuration.sparkleAllowsDowngrades, privacy: .public), mounted_dmg=\(configuration.appearsMountedFromDiskImage, privacy: .public), translocated=\(configuration.appearsTranslocated, privacy: .public), package_manager_managed=\(configuration.appearsPackageManagerManaged, privacy: .public), feed_configured=\(configuration.sparkleFeedURL != nil, privacy: .public), key_configured=\(configuration.sparklePublicEdKey != nil, privacy: .public), can_check=\(self.canCheckForUpdates, privacy: .public), unavailable_reason=\(self.unavailableReason?.rawValue ?? "none", privacy: .public)"
        )
    }

    var unavailableReasonLocalizationKey: String? {
        unavailableReason?.localizationKey
    }

    func checkForUpdates() {
        guard !ResearchFixtureSafetyPolicy.blocksLiveOperations() else {
            updateLogger.info("Ignoring Helm update check while a research fixture is active")
            return
        }
        guard canCheckForUpdates else {
            updateLogger.warning(
                "Ignoring manual update check request because updater is unavailable. reason=\(self.unavailableReason?.rawValue ?? "unknown", privacy: .public)"
            )
            return
        }
        guard !isCheckingForUpdates else {
            updateLogger.info("Ignoring manual update check request because a check is already in progress.")
            return
        }
        guard networkAvailable else {
            pendingCheckKind = .userInitiated
            updateLogger.info("Deferring manual Helm update check until connectivity returns.")
            return
        }

        updateLogger.info("Manual update check requested.")
        isCheckingForUpdates = true
        driver.checkForUpdates()
    }

    func refreshUpdateAvailability() {
        guard !ResearchFixtureSafetyPolicy.blocksLiveOperations() else { return }
        guard canCheckForUpdates, !isCheckingForUpdates, driver.canCheckForUpdates else { return }
        guard networkAvailable else {
            if pendingCheckKind != .userInitiated {
                pendingCheckKind = .information
            }
            updateLogger.info("Deferring information-only Helm update check until connectivity returns.")
            return
        }
        updateLogger.info("Information-only Helm update check requested.")
        isCheckingForUpdates = true
        driver.checkForUpdateInformation()
    }

    func setAutoCheckEnabled(_ enabled: Bool) {
        guard canCheckForUpdates else { return }
        UserDefaults.standard.set(enabled, forKey: Self.autoCheckEnabledKey)
        autoCheckEnabled = enabled
        scheduleAutomaticCheck()
        updateLogger.info("Automatic Helm update checks set to \(enabled, privacy: .public)")
    }

    func setCheckFrequencyMinutes(_ minutes: Int) {
        guard canCheckForUpdates else { return }
        let supportedMinutes = Self.supportedFrequencyMinutes(for: TimeInterval(minutes * 60))
        UserDefaults.standard.set(supportedMinutes, forKey: Self.checkFrequencyMinutesKey)
        checkFrequencyMinutes = supportedMinutes
        scheduleAutomaticCheck()
        updateLogger.info(
            "Automatic Helm update check interval set to \(supportedMinutes, privacy: .public) minutes"
        )
    }

    func setPrereleaseUpdatesEnabled(_ enabled: Bool) {
        guard canCheckForUpdates else { return }
        UserDefaults.standard.set(enabled, forKey: Self.prereleaseUpdatesEnabledKey)
        prereleaseUpdatesEnabled = enabled
        availableUpdate = nil
        driver.setPrereleaseUpdatesEnabled(enabled)
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            self?.refreshUpdateAvailability()
        }
        updateLogger.info("Prerelease Helm updates set to \(enabled, privacy: .public)")
    }

    func setIncludeHelmInUpgradeAll(_ enabled: Bool) {
        guard canCheckForUpdates else { return }
        UserDefaults.standard.set(enabled, forKey: Self.includeHelmInUpgradeAllKey)
        includeHelmInUpgradeAll = enabled
        updateLogger.info("Include Helm in Upgrade All set to \(enabled, privacy: .public)")
    }

    func refreshState() {
        scheduleAutomaticCheck()
    }

    func setNetworkAvailable(_ available: Bool) {
        guard networkAvailable != available else { return }
        networkAvailable = available
        if !available {
            automaticCheckTimer?.invalidate()
            automaticCheckTimer = nil
            return
        }

        let pending = pendingCheckKind
        pendingCheckKind = nil
        switch pending {
        case .userInitiated:
            checkForUpdates()
        case .background, .information:
            refreshUpdateAvailability()
        case nil:
            scheduleAutomaticCheck()
        }
    }

    private func finishCheck(checkKind: AppUpdateCheckKind, error: Error?) {
        isCheckingForUpdates = false
        let checkDate = Date()
        lastCheckDate = checkDate
        UserDefaults.standard.set(checkDate.timeIntervalSince1970, forKey: Self.lastCheckDateKey)
        if let error {
            updateLogger.error(
                "Helm update check finished with an error. kind=\(checkKind.rawValue, privacy: .public), error=\(error.localizedDescription, privacy: .public)"
            )
        }
        scheduleAutomaticCheck()
    }

    private func scheduleAutomaticCheck() {
        automaticCheckTimer?.invalidate()
        automaticCheckTimer = nil
        guard !ResearchFixtureSafetyPolicy.blocksLiveOperations() else { return }
        guard networkAvailable, let delay = AppUpdateSchedulePolicy.nextCheckDelay(
            canCheckForUpdates: canCheckForUpdates,
            autoCheckEnabled: autoCheckEnabled,
            lastCheckDate: lastCheckDate,
            frequencyMinutes: checkFrequencyMinutes
        ) else { return }
        let timer = Timer(timeInterval: delay, repeats: false) { [weak self] _ in
            self?.refreshUpdateAvailability()
        }
        timer.tolerance = min(max(delay * 0.05, 1), 300)
        RunLoop.main.add(timer, forMode: .common)
        automaticCheckTimer = timer
    }

    private static func supportedFrequencyMinutes(for interval: TimeInterval) -> Int {
        let requested = max(Int(interval / 60), 60)
        let supported = [60, 1_440, 10_080, 43_800]
        return supported.min(by: { abs($0 - requested) < abs($1 - requested) }) ?? 1_440
    }

    private struct AppUpdateDriverSelection {
        let driver: AppUpdateDriver
        let unavailableReason: AppUpdateUnavailableReason?
    }

    private static func makeDriver(
        for configuration: AppUpdateConfiguration,
        prereleaseUpdatesEnabled: Bool
    ) -> AppUpdateDriverSelection {
        if let failure = configuration.eligibilityFailureReason {
            return AppUpdateDriverSelection(
                driver: NoopAppUpdateDriver(),
                unavailableReason: mapFailureReason(failure)
            )
        }

        #if canImport(Sparkle)
        return AppUpdateDriverSelection(
            driver: SparkleAppUpdateDriver(
                configuration: configuration,
                prereleaseUpdatesEnabled: prereleaseUpdatesEnabled
            ),
            unavailableReason: nil
        )
        #else
        updateLogger.warning(
            "Sparkle build flag enabled for Developer ID channel, but Sparkle framework is unavailable."
        )
        return AppUpdateDriverSelection(
            driver: NoopAppUpdateDriver(),
            unavailableReason: .sparkleFrameworkUnavailable
        )
        #endif
    }

    private static func mapFailureReason(_ reason: AppUpdateEligibilityFailure) -> AppUpdateUnavailableReason {
        switch reason {
        case .channelNotSupported:
            return .channelNotSupported
        case .sparkleDisabled:
            return .sparkleDisabled
        case .downgradesEnabled:
            return .downgradesEnabled
        case .missingSparkleConfig:
            return .missingSparkleConfig
        case .insecureSparkleFeed:
            return .insecureSparkleFeed
        case .bundleVersionMetadataMismatch:
            return .bundleVersionMetadataMismatch
        case .ineligibleInstallLocation:
            return .ineligibleInstallLocation
        case .packageManagerManagedInstall:
            return .packageManagerManagedInstall
        }
    }
}
