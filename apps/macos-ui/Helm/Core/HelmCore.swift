import AppKit
import Combine
import Foundation
import Network
import os.log

enum HelmNetworkAvailability: Equatable {
    case unknown
    case available
    case unavailable
}

private let logger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core")

struct CorePackageRef: Codable {
    let manager: String
    let name: String
}

struct CoreInstalledPackage: Codable {
    let package: CorePackageRef
    let packageIdentifier: String?
    let installedVersion: String?
    let pinned: Bool
    let runtimeState: PackageRuntimeState?
}

struct CoreOutdatedPackage: Codable {
    let package: CorePackageRef
    let packageIdentifier: String?
    let installedVersion: String?
    let candidateVersion: String
    let pinned: Bool
    let restartRequired: Bool
    let runtimeState: PackageRuntimeState?
}

struct CoreTaskRecord: Codable {
    let id: UInt64
    let manager: String
    let taskType: String
    let status: String
    let label: String?
    let labelKey: String?
    let labelArgs: [String: String]?
}

struct CoreTaskOutputRecord: Codable {
    let taskId: UInt64
    let command: String?
    let cwd: String?
    let programPath: String?
    let pathSnippet: String?
    let startedAtUnixMs: Int64?
    let finishedAtUnixMs: Int64?
    let durationMs: UInt64?
    let exitCode: Int32?
    let terminationReason: String?
    let errorCode: String?
    let errorMessage: String?
    let stdout: String?
    let stderr: String?
}

struct CoreTaskLogRecord: Codable, Identifiable {
    let id: UInt64
    let taskId: UInt64
    let manager: String
    let taskType: String
    let status: String?
    let level: String
    let message: String
    let createdAtUnix: Int64

    var createdAtDate: Date {
        Date(timeIntervalSince1970: TimeInterval(createdAtUnix))
    }
}

struct CoreTaskTimeoutPrompt: Codable, Identifiable {
    let taskId: UInt64
    let manager: String
    let taskType: String
    let action: String
    let requestedAtUnixMs: Int64
    let graceSeconds: UInt64
    let suggestedExtensionSeconds: UInt64

    var id: String {
        "\(taskId):\(requestedAtUnixMs)"
    }
}

enum ManagerDetectionDiagnosticReason {
    case detected
    case notDetected
    case inconsistent
    case inProgress
    case failed
    case disabled
    case notImplemented
    case neverChecked
}

struct ManagerDetectionDiagnostics {
    let reason: ManagerDetectionDiagnosticReason
    let latestTaskId: UInt64?
    let latestTaskStatus: String?
}

struct CoreErrorAttribution: Codable {
    let source: String
    let action: String
    let managerId: String?
    let taskType: String?
    let occurredAtUnix: Int64
}

struct CoreSearchResult: Codable {
    let manager: String
    let name: String
    let packageIdentifier: String?
    let version: String?
    let summary: String?
    let sourceManager: String
}

struct CoreRustupToolchainDetailEntry: Codable, Hashable, Identifiable {
    let name: String
    let installed: Bool

    var id: String { name }
}

struct CoreRustupToolchainDetail: Codable, Hashable {
    let toolchain: String
    let currentProfile: String?
    let overridePaths: [String]
    let components: [CoreRustupToolchainDetailEntry]
    let targets: [CoreRustupToolchainDetailEntry]
}

struct CoreUpgradePlanStep: Codable, Identifiable, Equatable {
    let stepId: String
    let orderIndex: UInt64
    let managerId: String
    let authority: String
    let action: String
    let packageName: String
    let reasonLabelKey: String
    let reasonLabelArgs: [String: String]
    let status: String

    var id: String { stepId }
}

struct UpgradePlanTaskProjection {
    let stepId: String
    let taskId: UInt64
    let status: String
    let managerId: String
    let labelKey: String?
}

struct UpgradePlanFailureGroup: Identifiable {
    let id: String
    let managerId: String
    let stepIds: [String]
    let packageNames: [String]
}

enum HomebrewKegPolicyOverride: String, Codable {
    case keep
    case cleanup
}

struct CorePackageKegPolicy: Codable {
    let managerId: String
    let packageName: String
    let policy: HomebrewKegPolicyOverride
}

struct CorePackageManagerPreference: Codable {
    let packageFamilyKey: String
    let managerId: String
}

enum KegPolicySelection {
    case useGlobal
    case keep
    case cleanup
}

struct ManagerStatus: Codable {
    let managerId: String
    let detected: Bool
    let version: String?
    let executablePath: String?
    let executablePaths: [String]?
    let defaultExecutablePath: String?
    let selectedExecutablePath: String?
    let selectedInstallMethod: String?
    let installMethodOptions: [ManagerInstallMethodStatus]?
    let timeoutHardSeconds: Int?
    let timeoutIdleSeconds: Int?
    let enabled: Bool
    let isImplemented: Bool
    let isOptional: Bool
    let isDetectionOnly: Bool
    let supportsRemoteSearch: Bool
    let supportsPackageInstall: Bool
    let supportsPackageUninstall: Bool
    let supportsPackageUpgrade: Bool
    let packageStateIssues: [ManagerPackageStateIssue]?
    let isEligible: Bool?
    let ineligibleReasonCode: String?
    let ineligibleReasonMessage: String?
    let ineligibleServiceErrorKey: String?
    let installInstances: [ManagerInstallInstanceStatus]?
    let installInstanceCount: Int?
    let multiInstanceState: String?
    let multiInstanceAcknowledged: Bool?
    let multiInstanceFingerprint: String?
    let activeProvenance: String?
    let activeConfidence: Double?
    let activeDecisionMargin: Double?
    let activeAutomationLevel: String?
    let activeUninstallStrategy: String?
    let activeUpdateStrategy: String?
    let activeRemediationStrategy: String?
    let activeExplanationPrimary: String?
    let activeExplanationSecondary: String?
    let competingProvenance: String?
    let competingConfidence: Double?
}

struct ManagerPackageStateIssue: Codable {
    let sourceManagerId: String
    let packageName: String
    let issueCode: String
    let findingCode: String?
    let fingerprint: String?
    let severity: String?
    let summary: String?
    let evidencePrimary: String?
    let evidenceSecondary: String?
    let knowledgeSource: String?
    let knowledgeVersion: String?
    let repairOptions: [ManagerPackageStateIssueRepairOption]?
}

struct RepairOptionContentKeys: Codable {
    let title: String
    let description: String
    let impact: String?
    let guidance: String?
}

struct ManagerPackageStateIssueRepairOption: Codable {
    let optionId: String
    let action: String
    let title: String
    let description: String
    let contentKeys: RepairOptionContentKeys?
    let recommended: Bool
    let requiresConfirmation: Bool
    let automationLevel: String
}

struct ManagerInstallMethodStatus: Codable {
    let methodId: String
    let recommendationRank: Int
    let recommendationReason: String?
    let policyTag: String
    let executablePathHints: [String]?
    let packageHints: [String]?
}

struct ManagerInstallInstanceStatus: Codable, Identifiable {
    let instanceId: String
    let identityKind: String
    let identityValue: String
    let displayPath: String
    let canonicalPath: String?
    let aliasPaths: [String]
    let isActive: Bool
    let version: String?
    let provenance: String
    let confidence: Double
    let decisionMargin: Double?
    let automationLevel: String
    let uninstallStrategy: String
    let updateStrategy: String
    let remediationStrategy: String
    let explanationPrimary: String?
    let explanationSecondary: String?
    let competingProvenance: String?
    let competingConfidence: Double?

    var id: String { instanceId }
}

struct ManagerUninstallImpactPath: Codable {
    let path: String
    let exists: Bool
}

struct ManagerUninstallPreview: Codable {
    let requestedManagerId: String
    let targetManagerId: String
    let packageName: String
    let strategy: String
    let provenance: String?
    let automationLevel: String?
    let confidence: Double?
    let decisionMargin: Double?
    let explanationPrimary: String?
    let explanationSecondary: String?
    let competingProvenance: String?
    let competingConfidence: Double?
    let filesRemoved: [ManagerUninstallImpactPath]
    let directoriesRemoved: [ManagerUninstallImpactPath]
    let secondaryEffects: [String]
    let summaryLines: [String]
    let blastRadiusScore: Int
    let requiresYes: Bool
    let confidenceRequiresConfirmation: Bool
    let unknownProvenance: Bool
    let unknownOverrideRequired: Bool
    let usedUnknownOverride: Bool
    let legacyFallbackUsed: Bool
    let readOnlyBlocked: Bool
}

enum ManagerRustupInstallSource: String, Codable {
    case officialDownload
    case existingBinaryPath
}

enum ManagerMiseInstallSource: String, Codable {
    case officialDownload
    case existingBinaryPath
}

enum ManagerMiseUninstallCleanupMode: String, Codable {
    case managerOnly
    case fullCleanup
}

enum ManagerMiseUninstallConfigRemoval: String, Codable {
    case keepConfig
    case removeConfig
}

enum ManagerHomebrewUninstallCleanupMode: String, Codable {
    case managerOnly
    case fullCleanup
}

struct ManagerInstallActionOptions: Codable {
    let rustupInstallSource: ManagerRustupInstallSource?
    let rustupBinaryPath: String?
    let miseInstallSource: ManagerMiseInstallSource?
    let miseBinaryPath: String?
    let completePostInstallSetupAutomatically: Bool?
}

struct ManagerUninstallActionOptions: Codable {
    let allowUnknownProvenance: Bool?
    let homebrewCleanupMode: ManagerHomebrewUninstallCleanupMode?
    let miseCleanupMode: ManagerMiseUninstallCleanupMode?
    let miseConfigRemoval: ManagerMiseUninstallConfigRemoval?
    let removeHelmManagedShellSetup: Bool?
}

struct PackageUninstallPreview: Codable {
    let managerId: String
    let packageName: String
    let filesRemoved: [ManagerUninstallImpactPath]
    let directoriesRemoved: [ManagerUninstallImpactPath]
    let secondaryEffects: [String]
    let summaryLines: [String]
    let blastRadiusScore: Int
    let requiresYes: Bool
    let confidenceRequiresConfirmation: Bool
    let managerProvenance: String?
    let managerAutomationLevel: String?
    let managerUninstallStrategy: String?
    let explanationPrimary: String?
    let explanationSecondary: String?
    let competingProvenance: String?
    let competingConfidence: Double?
}

final class HelmManagersState: ObservableObject {
    @Published private(set) var authoritativeManagers: [ManagerInfo] = []
    @Published private(set) var standardManagers: [ManagerInfo] = []
    @Published private(set) var guardedManagers: [ManagerInfo] = []
    @Published private(set) var managerStatusesById: [String: ManagerStatus] = [:]
    @Published private(set) var managerOperationsById: [String: String] = [:]
    @Published private(set) var installedCountByManager: [String: Int] = [:]
    @Published private(set) var outdatedCountByManager: [String: Int] = [:]

    func apply(
        authoritativeManagers: [ManagerInfo],
        standardManagers: [ManagerInfo],
        guardedManagers: [ManagerInfo],
        managerStatusesById: [String: ManagerStatus],
        managerOperationsById: [String: String],
        installedCountByManager: [String: Int],
        outdatedCountByManager: [String: Int]
    ) {
        self.authoritativeManagers = authoritativeManagers
        self.standardManagers = standardManagers
        self.guardedManagers = guardedManagers
        self.managerStatusesById = managerStatusesById
        self.managerOperationsById = managerOperationsById
        self.installedCountByManager = installedCountByManager
        self.outdatedCountByManager = outdatedCountByManager
    }
}

final class HelmCore: ObservableObject {
    static let shared = HelmCore()
    static let currentLicenseTermsVersion = AppUpdateConfiguration.currentLicenseTermsVersion
    static let helmSelfUpdateManagerId = UpgradePreviewPlanner.helmSelfUpdateManagerId
    static let helmSelfUpdatePackageId = "helm-self-update"

    private static let onboardingCompletedKey = "hasCompletedOnboarding"
    private static let acceptedLicenseTermsVersionKey = "acceptedLicenseTermsVersion"
    private static let acceptedLicenseTermsAcceptedAtUnixKey = "acceptedLicenseTermsAcceptedAtUnix"
    static let launchAtLoginEnabledKey = "launchAtLoginEnabled"
    static let notificationsEnabledKey = "notificationsEnabled"
    static let managerPriorityOverridesKey = "managerPriorityOverrides"

    @Published var isInitialized = false
    @Published var isConnected = false {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published private(set) var networkAvailability: HelmNetworkAvailability = .unknown {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published var isRefreshing = false {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published var isSearching = false
    @Published var searchText: String = "" {
        didSet { onSearchTextChanged(searchText) }
    }
    @Published var installedPackages: [PackageItem] = [] {
        didSet {
            invalidateKnownPackageCaches()
            scheduleDerivedViewStateRefresh()
        }
    }
    @Published var outdatedPackages: [PackageItem] = [] {
        didSet {
            invalidateKnownPackageCaches()
            scheduleDerivedViewStateRefresh()
        }
    }
    @Published var activeTasks: [TaskItem] = [] {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published var taskTimeoutPrompts: [CoreTaskTimeoutPrompt] = [] {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published var searchResults: [PackageItem] = []
    @Published var cachedAvailablePackages: [PackageItem] = [] {
        didSet {
            invalidateKnownPackageCaches()
            scheduleDerivedViewStateRefresh()
        }
    }
    @Published var upgradePlanSteps: [CoreUpgradePlanStep] = []
    @Published var upgradePlanTaskProjectionByStepId: [String: UpgradePlanTaskProjection] = [:]
    @Published var upgradePlanFailureGroups: [UpgradePlanFailureGroup] = []
    @Published var upgradePlanCompletion: UpgradePlanCompletion?
    @Published var upgradePlanAllowOsUpdates: Bool = false
    @Published var upgradePlanIncludePinned: Bool = false
    @Published var upgradePlanPreviewRevisionState = UpgradePlanPreviewRevisionState()
    @Published var scopedUpgradePlanRunInProgress: Bool = false
    @Published var detectedManagers: Set<String> = [] {
        didSet {
            invalidateKnownPackageCaches()
            scheduleDerivedViewStateRefresh()
        }
    }
    @Published var managerStatuses: [String: ManagerStatus] = [:] {
        didSet {
            invalidateKnownPackageCaches()
            scheduleDerivedViewStateRefresh()
        }
    }
    @Published var managerPriorityOverrides: [String: Int] = HelmCore.loadManagerPriorityOverrides() {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published var managerOperations: [String: String] = [:] {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published var verifyingManagerIds: Set<String> = [] {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published var pinActionPackageIds: Set<String> = []
    @Published var upgradeActionPackageIds: Set<String> = []
    @Published var installActionPackageIds: Set<String> = []
    @Published var uninstallActionPackageIds: Set<String> = []
    @Published var packageDescriptionLoadingIds: Set<String> = []
    @Published var packageDescriptionUnavailableIds: Set<String> = []
    @Published var packageDescriptionSummaryByKey: [String: String] = [:]
    @Published var rustupToolchainDetailsByKey: [String: CoreRustupToolchainDetail] = [:]
    @Published var rustupToolchainDetailLoadingKeys: Set<String> = []
    @Published var rustupToolchainDetailUnavailableKeys: Set<String> = []
    @Published var rustupToolchainActionInFlightKeys: Set<String> = []
    @Published var onboardingDetectionInProgress: Bool = false
    var onboardingDetectionStatusSyncRequested: Bool = false
    @Published var homebrewKegAutoCleanupEnabled: Bool = false
    @Published var packageKegPolicyOverrides: [String: HomebrewKegPolicyOverride] = [:]
    @Published var packageManagerPreferencesByFamilyKey: [String: String] = [:]
    @Published var safeModeEnabled: Bool = false
    @Published var lastError: String?
    @Published var lastErrorAttribution: CoreErrorAttribution?
    @Published var selectedManagerFilter: String?
    @Published var hasCompletedOnboarding: Bool = UserDefaults.standard.bool(forKey: HelmCore.onboardingCompletedKey) {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published var acceptedLicenseTermsVersion: String? = UserDefaults.standard.string(
        forKey: HelmCore.acceptedLicenseTermsVersionKey
    ) {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    @Published var acceptedLicenseTermsAcceptedAtUnix: Int64? = {
        guard let value = UserDefaults.standard.object(
            forKey: HelmCore.acceptedLicenseTermsAcceptedAtUnixKey
        ) as? NSNumber else {
            return nil
        }
        return value.int64Value
    }()
    @Published var launchAtLoginEnabled: Bool = UserDefaults.standard.bool(
        forKey: HelmCore.launchAtLoginEnabledKey
    )
    @Published var notificationsEnabled: Bool = {
        AppNotificationPreference.resolvedEnabled(
            storedValue: UserDefaults.standard.object(forKey: HelmCore.notificationsEnabledKey)
        )
    }()
    @Published var helmCliShimInstalled: Bool = false
    @Published var helmCliBundledAvailable: Bool = false
    @Published var helmCliShimOperationInProgress: Bool = false
    @Published var helmCliShimStatusMessage: String?
    @Published var helmCliShimPath: String = HelmCore.defaultHelmCliShimURL().path
    @Published var helmCliBundledPath: String?

    let overviewState = HelmOverviewState()
    let firstRunPresentationModel = FirstRunPresentationModel()
    let managersState = HelmManagersState()

    var timer: Timer?
    var connection: NSXPCConnection?
    var lastRefreshTrigger: Date?
    var lastCompletedRefreshAt: Date? {
        didSet { scheduleDerivedViewStateRefresh() }
    }
    var searchDebounceTimer: Timer?
    var localSearchRequestGeneration: UInt64 = 0
    var activeRemoteSearchTaskIds: Set<Int64> = []
    var managerActionTaskDescriptions: [UInt64: String] = [:]
    var managerActionTaskByManager: [String: UInt64] = [:]
    var managerActionTaskTypes: [UInt64: String] = [:]
    var managerActionTaskSubmittedAt: [UInt64: Date] = [:]
    var managerVerificationAnchorTaskIdByManager: [String: UInt64] = [:]
    var managerVerificationStartedAtByManager: [String: Date] = [:]
    var managerPostInstallSetupHandledTaskIds: [String: UInt64] = [:]
    var localManagerActionTasks: [String: TaskItem] = [:]
    var localManagerActionTaskCreatedAt: [String: Date] = [:]
    var upgradeActionTaskByPackage: [String: UInt64] = [:]
    var installActionTaskByPackage: [String: UInt64] = [:]
    var installActionNormalizedNameByPackageId: [String: String] = [:]
    var installActionTargetPackageById: [String: PackageItem] = [:]
    var uninstallActionTaskByPackage: [String: UInt64] = [:]
    var rustupToolchainActionTaskByKey: [String: UInt64] = [:]
    var rustupToolchainActionPackageByKey: [String: PackageItem] = [:]
    var rustupToolchainActionSubmittedAtByKey: [String: Date] = [:]
    var descriptionLookupTaskIdsByPackage: [String: Set<UInt64>] = [:]
    var descriptionLookupStartedAtByPackage: [String: Date] = [:]
    var descriptionLookupPackageById: [String: PackageItem] = [:]
    var lastObservedTaskId: UInt64 = 0
    var onboardingDetectionAnchorTaskId: UInt64 = 0
    var onboardingDetectionPendingManagers: Set<String> = []
    var onboardingDetectionStartedAt: Date?
    var latestCoreTasksSnapshot: [CoreTaskRecord] = []
    var previousFailedTaskCount: Int = 0
    var previousRefreshState: Bool = false
    var scopedUpgradeWorkflowId: String?
    var pendingHelmSelfUpdateWorkflowId: String?
    var scopedUpgradeWorkflowStartState = UpgradeWorkflowStartState()
    var scopedUpgradeWorkflowStatusReconciliationState = UpgradeWorkflowStatusReconciliationState()
    var upgradePlanCompletionTracker = UpgradePlanCompletionTracker()
    var scopedUpgradeWorkflowStatusCheckInFlight = false
    private var connectionGeneration: UInt64 = 0
    private var reconnectToken: UUID?
    private var reconnectPolicy = ServiceConnectionRetryPolicy()
    private var refreshRequestedWhileDisconnected = false
    private var refreshRequestedWhileOffline = false
    private let networkMonitor = NWPathMonitor()
    private let networkMonitorQueue = DispatchQueue(label: "com.jasoncavinder.Helm.network-monitor")
    private var lastTaskSnapshotRefreshAt: Date = .distantPast
    private var lastFullSnapshotRefreshAt: Date = .distantPast
    private var isPopoverVisibleForPolling = false
    private var isControlCenterVisibleForPolling = false
    private var derivedViewStateRefreshWorkItem: DispatchWorkItem?
    private var appUpdateAvailabilityCancellable: AnyCancellable?
    var cachedAllKnownPackagesUnsorted: [PackageItem]?
    var cachedAllKnownPackagesSorted: [PackageItem]?
    var cachedKnownPackageById: [String: PackageItem] = [:]
    private var packageDescriptionRenderCache: [String: PackageDescriptionRenderCacheEntry] = [:]
    private var packageDescriptionRenderCacheOrder: [String] = []
    private static let maxPackageDescriptionRenderCacheEntries = 256

    private enum PackageDescriptionRenderCacheEntry {
        case none
        case rendered(PackageDescriptionRenderer.RenderedDescription)
    }

    private init() {
        appUpdateAvailabilityCancellable = AppUpdateCoordinator.shared.$availableUpdate
            .receive(on: DispatchQueue.main)
            .sink { [weak self] availability in
                self?.syncHelmSelfUpdateAvailability(availability)
            }
        if WholeWorkflowResearchDatasetProvider.isSelected() {
            isInitialized = true
            isConnected = true
            networkAvailability = .available
            return
        }
        refreshHelmCliShimStatus()
        startNetworkMonitoring()
        setupConnection()
    }

    var networkOperationsAvailable: Bool {
        networkAvailability == .available
    }

    private func startNetworkMonitoring() {
        networkMonitor.pathUpdateHandler = { [weak self] path in
            let availability: HelmNetworkAvailability = path.status == .satisfied
                ? .available
                : .unavailable
            DispatchQueue.main.async {
                self?.applyNetworkAvailability(availability)
            }
        }
        networkMonitor.start(queue: networkMonitorQueue)
    }

    private func applyNetworkAvailability(_ availability: HelmNetworkAvailability) {
        guard networkAvailability != availability else { return }
        let previous = networkAvailability
        networkAvailability = availability
        AppUpdateCoordinator.shared.setNetworkAvailable(availability == .available)
        syncNetworkAvailabilityToService()

        if availability == .available {
            switch DeferredOfflineRefreshPolicy.disposition(
                networkIsAvailable: true,
                refreshRequestedWhileOffline: refreshRequestedWhileOffline,
                refreshIsInFlight: deferredOfflineRefreshIsInFlight
            ) {
            case .resumeNow:
                resumeDeferredOfflineRefreshIfReady()
            case .waitForCurrentRefresh:
                logger.info(
                    "Connectivity restored during an in-flight refresh; waiting to resume deferred network work"
                )
            case .none:
                break
            }
        } else if previous == .unknown, availability == .unavailable,
                  refreshRequestedWhileOffline {
            // Complete the local half of an initial refresh once the path is known.
            triggerRefresh()
        }
    }

    private func syncNetworkAvailabilityToService(_ completion: (() -> Void)? = nil) {
        guard networkAvailability != .unknown, let service = service() else {
            completion?()
            return
        }
        service.setNetworkAvailable(available: networkAvailability == .available) { success in
            if !success {
                logger.warning("Failed to synchronize network availability with HelmService")
            }
            DispatchQueue.main.async {
                completion?()
            }
        }
    }

    func resumeDeferredOfflineRefreshIfReady() {
        guard DeferredOfflineRefreshPolicy.disposition(
            networkIsAvailable: networkAvailability == .available,
            refreshRequestedWhileOffline: refreshRequestedWhileOffline,
            refreshIsInFlight: deferredOfflineRefreshIsInFlight
        ) == .resumeNow else {
            return
        }

        refreshRequestedWhileOffline = false
        logger.info("Connectivity restored; resuming one deferred refresh")
        triggerRefresh()
    }

    private var deferredOfflineRefreshIsInFlight: Bool {
        DeferredOfflineRefreshPolicy.refreshIsInFlight(
            presentationIsRefreshing: isRefreshing,
            tasks: latestCoreTasksSnapshot.map {
                DeferredOfflineRefreshTaskState(taskType: $0.taskType, status: $0.status)
            }
        )
    }

    private static func loadManagerPriorityOverrides() -> [String: Int] {
        guard let data = UserDefaults.standard.data(forKey: managerPriorityOverridesKey) else {
            return [:]
        }
        guard let decoded = try? JSONDecoder().decode([String: Int].self, from: data) else {
            return [:]
        }
        return decoded
    }

    func invalidateKnownPackageCaches() {
        cachedAllKnownPackagesUnsorted = nil
        cachedAllKnownPackagesSorted = nil
        cachedKnownPackageById = [:]
    }

    static func requiresLicenseTermsAcceptance(
        channel: HelmDistributionChannel,
        acceptedVersion: String?
    ) -> Bool {
        AppUpdateConfiguration.requiresLicenseTermsAcceptance(
            channel: channel,
            acceptedVersion: acceptedVersion
        )
    }

    var requiresLicenseTermsAcceptance: Bool {
        Self.requiresLicenseTermsAcceptance(
            channel: HelmDistributionChannel.from(),
            acceptedVersion: acceptedLicenseTermsVersion
        )
    }

    private func persistSharedOnboardingCompleted(_ completed: Bool) {
        guard let service = service() else { return }
        service.setSharedOnboardingCompleted(completed: completed) { [weak self] success in
            guard !success else { return }
            self?.recordLastError(
                source: "core.settings",
                action: "setSharedOnboardingCompleted",
                taskType: "settings"
            )
        }
    }

    private func persistSharedAcceptedLicenseTermsVersion(_ version: String?) {
        guard let service = service() else { return }
        service.setSharedAcceptedLicenseTermsVersion(version: version) { [weak self] success in
            guard !success else { return }
            self?.recordLastError(
                source: "core.settings",
                action: "setSharedAcceptedLicenseTermsVersion",
                taskType: "settings"
            )
        }
    }

    private func applyOnboardingStateLocally(completed: Bool, acceptedVersion: String?) {
        if completed {
            UserDefaults.standard.set(true, forKey: Self.onboardingCompletedKey)
        } else {
            UserDefaults.standard.removeObject(forKey: Self.onboardingCompletedKey)
        }
        hasCompletedOnboarding = completed

        if let acceptedVersion {
            UserDefaults.standard.set(acceptedVersion, forKey: Self.acceptedLicenseTermsVersionKey)
            acceptedLicenseTermsVersion = acceptedVersion

            if let existing = acceptedLicenseTermsAcceptedAtUnix {
                UserDefaults.standard.set(existing, forKey: Self.acceptedLicenseTermsAcceptedAtUnixKey)
            } else {
                let now = Int64(Date().timeIntervalSince1970)
                UserDefaults.standard.set(now, forKey: Self.acceptedLicenseTermsAcceptedAtUnixKey)
                acceptedLicenseTermsAcceptedAtUnix = now
            }
        } else {
            UserDefaults.standard.removeObject(forKey: Self.acceptedLicenseTermsVersionKey)
            UserDefaults.standard.removeObject(forKey: Self.acceptedLicenseTermsAcceptedAtUnixKey)
            acceptedLicenseTermsVersion = nil
            acceptedLicenseTermsAcceptedAtUnix = nil
        }
    }

    private func syncOnboardingStateWithSharedStore() {
        guard let service = service() else { return }

        service.getSharedOnboardingState { [weak self] remoteCompleted, remoteAcceptedVersionRaw in
            guard let self else { return }
            DispatchQueue.main.async {
                let localCompleted = self.hasCompletedOnboarding
                let localAcceptedVersion = self.acceptedLicenseTermsVersion?
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                let normalizedLocalAcceptedVersion = (localAcceptedVersion?.isEmpty == false)
                    ? localAcceptedVersion
                    : nil
                let remoteAcceptedVersion = remoteAcceptedVersionRaw?
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                let normalizedRemoteAcceptedVersion = (remoteAcceptedVersion?.isEmpty == false)
                    ? remoteAcceptedVersion
                    : nil
                let remoteHasState = remoteCompleted || normalizedRemoteAcceptedVersion != nil
                let localHasState = localCompleted || normalizedLocalAcceptedVersion != nil

                if remoteHasState {
                    self.applyOnboardingStateLocally(
                        completed: remoteCompleted,
                        acceptedVersion: normalizedRemoteAcceptedVersion
                    )
                    return
                }

                if localHasState {
                    self.persistSharedOnboardingCompleted(localCompleted)
                    self.persistSharedAcceptedLicenseTermsVersion(normalizedLocalAcceptedVersion)
                }
            }
        }
    }

    func setupConnection() {
        guard !WholeWorkflowResearchDatasetProvider.isSelected() else { return }
        guard connection == nil else { return }

        reconnectToken = nil
        reconnectPolicy.beginConnectionAttempt()
        connectionGeneration &+= 1
        let generation = connectionGeneration
        let candidate = NSXPCConnection(serviceName: "app.jasoncavinder.Helm.HelmService")
        candidate.remoteObjectInterface = NSXPCInterface(with: HelmServiceProtocol.self)
        candidate.invalidationHandler = { [weak self] in
            logger.error("XPC connection invalidated")
            DispatchQueue.main.async {
                self?.handleConnectionFailure(generation: generation)
            }
        }
        candidate.interruptionHandler = { [weak self] in
            logger.error("XPC connection interrupted")
            DispatchQueue.main.async {
                self?.handleConnectionFailure(generation: generation)
            }
        }
        candidate.resume()
        connection = candidate
        isInitialized = true

        guard let service = candidate.remoteObjectProxyWithErrorHandler({ [weak self] error in
            logger.error("XPC connection handshake failed: \(error.localizedDescription)")
            DispatchQueue.main.async {
                self?.handleConnectionFailure(generation: generation)
            }
        }) as? HelmServiceProtocol else {
            handleConnectionFailure(generation: generation)
            return
        }

        withTimeout(
            5,
            source: "core.xpc",
            action: "connectionHandshake",
            taskType: "connection",
            operation: { completion in
                service.getSafeMode { enabled in
                    completion(enabled)
                }
            },
            fallback: nil
        ) { [weak self] enabled in
            guard let self else { return }
            guard let enabled else {
                logger.error("XPC connection handshake timed out")
                self.handleConnectionFailure(generation: generation)
                return
            }
            DispatchQueue.main.async {
                self.completeConnectionHandshake(
                    generation: generation,
                    safeModeEnabled: enabled
                )
            }
        }
    }

    private func completeConnectionHandshake(generation: UInt64, safeModeEnabled: Bool) {
        guard generation == connectionGeneration, connection != nil else { return }

        logger.info("XPC connection handshake succeeded")
        reconnectToken = nil
        reconnectPolicy.markConnected()
        self.safeModeEnabled = safeModeEnabled
        isConnected = true

        if timer == nil {
            startPolling()
        }
        fetchHomebrewKegAutoCleanup()
        fetchPackageKegPolicies()
        fetchPackageManagerPreferences()
        syncOnboardingStateWithSharedStore()
        syncNetworkAvailabilityToService()
        scheduleDerivedViewStateRefresh()

        if refreshRequestedWhileDisconnected {
            refreshRequestedWhileDisconnected = false
            triggerRefresh()
        }
    }

    private func handleConnectionFailure(generation: UInt64) {
        guard generation == connectionGeneration, let failedConnection = connection else {
            return
        }

        failedConnection.invalidationHandler = nil
        failedConnection.interruptionHandler = nil
        failedConnection.invalidate()
        connection = nil
        if isConnected {
            isConnected = false
        }
        clearSearchState()
        scheduleReconnection()
    }

    func scheduleReconnection() {
        guard connection == nil, let delay = reconnectPolicy.scheduleReconnect() else {
            return
        }

        let token = UUID()
        reconnectToken = token
        logger.info("Scheduling reconnection in \(delay)s (attempt \(self.reconnectPolicy.attempt))")
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self, self.reconnectToken == token, self.connection == nil else {
                return
            }
            self.reconnectToken = nil
            logger.info("Attempting to reconnect...")
            self.setupConnection()
        }
    }

    func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.performPollingTick()
        }
    }

    private func performPollingTick() {
        let now = Date()
        let hasInFlightWork = isRefreshing
            || activeTasks.contains(where: \.isRunning)
            || !activeRemoteSearchTaskIds.isEmpty
        let interactiveSurfaceVisible = isPopoverVisibleForPolling || isControlCenterVisibleForPolling

        let taskSnapshotInterval: TimeInterval = {
            if hasInFlightWork {
                return 1.0
            }
            if interactiveSurfaceVisible {
                return 2.0
            }
            return 5.0
        }()

        if now.timeIntervalSince(lastTaskSnapshotRefreshAt) >= taskSnapshotInterval {
            lastTaskSnapshotRefreshAt = now
            fetchTasks()
            fetchTaskTimeoutPrompts()
        }

        let fullSnapshotInterval: TimeInterval = {
            if hasInFlightWork {
                return 2.0
            }
            if interactiveSurfaceVisible {
                return 4.0
            }
            return 8.0
        }()

        guard now.timeIntervalSince(lastFullSnapshotRefreshAt) >= fullSnapshotInterval else {
            return
        }

        triggerFullSnapshotRefresh()
    }

    private func triggerFullSnapshotRefresh() {
        lastFullSnapshotRefreshAt = Date()
        fetchPackages()
        fetchOutdatedPackages()
        fetchManagerStatus()
        refreshCachedAvailablePackages()

        // Re-query local cache to pick up enriched results from remote search.
        let trimmedQuery = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmedQuery.isEmpty {
            fetchSearchResults(query: trimmedQuery)
        }
    }

    func service() -> HelmServiceProtocol? {
        guard !WholeWorkflowResearchDatasetProvider.isSelected() else { return nil }
        guard isConnected else { return nil }
        return connection?.remoteObjectProxy as? HelmServiceProtocol
    }

    /// Wraps an asynchronous XPC operation with a timeout.
    /// If the operation does not complete within `seconds`, the completion
    /// handler is called with `fallback` and the actual result is discarded.
    func withTimeout<T>(
        _ seconds: TimeInterval,
        source: String = "core.xpc",
        action: String = "unknown",
        managerId: String? = nil,
        taskType: String? = nil,
        operation: @escaping (@escaping (T?) -> Void) -> Void,
        fallback: T? = nil,
        completion: @escaping (T?) -> Void
    ) {
        let completed = DispatchSemaphore(value: 1)
        var hasCompleted = false

        let deadline = DispatchWorkItem { [weak self] in
            completed.wait()
            if !hasCompleted {
                hasCompleted = true
                completed.signal()
                logger.warning(
                    "XPC call timed out after \(seconds)s (source=\(source), action=\(action), manager=\(managerId ?? "none"), task_type=\(taskType ?? "none"))"
                )
                DispatchQueue.main.async {
                    self?.recordLastError(
                        source: source,
                        action: action,
                        managerId: managerId,
                        taskType: taskType
                    )
                    completion(fallback)
                }
            } else {
                completed.signal()
            }
        }
        DispatchQueue.global().asyncAfter(deadline: .now() + seconds, execute: deadline)

        operation { result in
            completed.wait()
            if !hasCompleted {
                hasCompleted = true
                completed.signal()
                deadline.cancel()
                completion(result)
            } else {
                completed.signal()
            }
        }
    }

    func recordLastError(
        message: String = L10n.Common.error.localized,
        source: String,
        action: String,
        managerId: String? = nil,
        taskType: String? = nil
    ) {
        let attribution = CoreErrorAttribution(
            source: source,
            action: action,
            managerId: managerId,
            taskType: taskType,
            occurredAtUnix: Int64(Date().timeIntervalSince1970)
        )

        DispatchQueue.main.async {
            self.lastError = message
            self.lastErrorAttribution = attribution
        }
    }

    func consumeLastServiceErrorKey(_ completion: @escaping (String?) -> Void) {
        guard let service = service() else {
            completion(nil)
            return
        }
        service.takeLastErrorKey { key in
            DispatchQueue.main.async {
                completion(key)
            }
        }
    }

    func triggerRefresh() {
        logger.info("triggerRefresh called")
        guard !ResearchFixtureSafetyPolicy.blocksLiveOperations() else {
            logger.info("Ignoring refresh while a research fixture is active")
            return
        }
        if networkAvailability == .unknown {
            logger.info("Deferring refresh until the initial network path is known")
            refreshRequestedWhileOffline = true
            return
        }
        if networkAvailability == .unavailable {
            refreshRequestedWhileOffline = true
        } else {
            AppUpdateCoordinator.shared.refreshUpdateAvailability()
        }
        guard isConnected else {
            logger.info("Deferring refresh until the service connection is verified")
            refreshRequestedWhileDisconnected = true
            return
        }
        self.lastRefreshTrigger = Date()
        self.lastTaskSnapshotRefreshAt = .distantPast
        self.lastFullSnapshotRefreshAt = .distantPast
        self.isRefreshing = true
        postAccessibilityAnnouncement(L10n.Common.refresh.localized)
        guard let service = connection?.remoteObjectProxyWithErrorHandler({ [weak self] error in
            logger.error("triggerRefresh XPC error: \(error.localizedDescription)")
            self?.completeFailedRefresh(action: "triggerRefresh.xpc")
        }) as? HelmServiceProtocol else {
            logger.error("triggerRefresh failed: service unavailable")
            completeFailedRefresh(action: "triggerRefresh.service_unavailable")
            return
        }
        syncNetworkAvailabilityToService { [weak self] in
            guard let self else { return }
            service.triggerRefresh { success in
                if !success {
                    logger.error("triggerRefresh failed")
                    self.completeFailedRefresh(action: "triggerRefresh")
                } else {
                    DispatchQueue.main.async {
                        self.triggerFullSnapshotRefresh()
                    }
                }
            }
        }
    }

    private func completeFailedRefresh(action: String) {
        recordLastError(source: "core", action: action, taskType: "refresh")
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.isRefreshing = false
            self.lastRefreshTrigger = nil
            self.completeOnboardingDetectionProgress()
            self.postAccessibilityAnnouncement(L10n.Common.error.localized)
        }
    }

    func triggerDetection() {
        logger.info("triggerDetection called")
        guard !WholeWorkflowResearchDatasetProvider.isSelected() else {
            logger.info("Ignoring detection while a research fixture is active")
            return
        }
        self.lastTaskSnapshotRefreshAt = .distantPast
        self.lastFullSnapshotRefreshAt = .distantPast

        service()?.triggerDetection { success in
            if !success {
                logger.error("triggerDetection failed")
                self.recordLastError(
                    source: "core",
                    action: "triggerDetection",
                    taskType: "detection"
                )
                DispatchQueue.main.async {
                    self.completeOnboardingDetectionProgress()
                }
            } else {
                DispatchQueue.main.async {
                    self.triggerFullSnapshotRefresh()
                }
            }
        }
    }

    func triggerDetection(for managerId: String, completion: ((Bool) -> Void)? = nil) {
        logger.info("triggerDetectionForManager called (manager=\(managerId, privacy: .public))")
        self.lastTaskSnapshotRefreshAt = .distantPast
        self.lastFullSnapshotRefreshAt = .distantPast

        service()?.triggerDetectionForManager(managerId: managerId) { success in
            if !success {
                logger.error("triggerDetectionForManager failed (manager=\(managerId, privacy: .public))")
                self.recordLastError(
                    source: "core",
                    action: "triggerDetectionForManager",
                    managerId: managerId,
                    taskType: "detection"
                )
            }
            DispatchQueue.main.async {
                completion?(success)
            }
        }
    }

    func setInteractiveSurfaceVisibility(
        popoverVisible: Bool,
        controlCenterVisible: Bool
    ) {
        DispatchQueue.main.async {
            self.isPopoverVisibleForPolling = popoverVisible
            self.isControlCenterVisibleForPolling = controlCenterVisible
            if popoverVisible || controlCenterVisible {
                self.lastTaskSnapshotRefreshAt = .distantPast
                self.lastFullSnapshotRefreshAt = .distantPast
            }
        }
    }

    func triggerOnboardingDetectionRefresh() {
        let visibleMaxTaskId = activeTasks
            .compactMap { UInt64($0.id) }
            .max() ?? 0
        onboardingDetectionAnchorTaskId = max(lastObservedTaskId, visibleMaxTaskId)

        let enabledImplementedManagers = Set(
            ManagerInfo.all
                .filter {
                    let status = managerStatuses[$0.id]
                    let isImplemented = status?.isImplemented ?? $0.isImplemented
                    let isEnabled = status?.enabled ?? true
                    return isImplemented && isEnabled
                }
                .map(\.id)
        )
        onboardingDetectionPendingManagers = enabledImplementedManagers
        onboardingDetectionStartedAt = Date()
        onboardingDetectionInProgress = !enabledImplementedManagers.isEmpty
        onboardingDetectionStatusSyncRequested = false

        triggerDetection()
    }

    func normalizedManagerName(_ raw: String) -> String {
        localizedManagerDisplayName(raw)
    }

    func completeOnboarding() {
        UserDefaults.standard.set(true, forKey: Self.onboardingCompletedKey)
        hasCompletedOnboarding = true
        persistSharedOnboardingCompleted(true)
    }

    func acceptCurrentLicenseTerms(acceptedAt: Date = Date()) {
        let acceptedAtUnix = Int64(acceptedAt.timeIntervalSince1970)
        UserDefaults.standard.set(
            Self.currentLicenseTermsVersion,
            forKey: Self.acceptedLicenseTermsVersionKey
        )
        UserDefaults.standard.set(
            acceptedAtUnix,
            forKey: Self.acceptedLicenseTermsAcceptedAtUnixKey
        )
        acceptedLicenseTermsVersion = Self.currentLicenseTermsVersion
        acceptedLicenseTermsAcceptedAtUnix = acceptedAtUnix
        persistSharedAcceptedLicenseTermsVersion(Self.currentLicenseTermsVersion)
    }

    func resetDatabase(completion: @escaping (Bool) -> Void) {
        // Stop polling during reset to prevent stale reads
        timer?.invalidate()
        timer = nil
        packageDescriptionRenderCache = [:]
        packageDescriptionRenderCacheOrder = []

        service()?.resetDatabase { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    self?.installedPackages = []
                    self?.outdatedPackages = []
                    self?.activeTasks = []
                    self?.taskTimeoutPrompts = []
                    self?.searchResults = []
                    self?.cachedAvailablePackages = []
                    self?.detectedManagers = []
                    self?.managerStatuses = [:]
                    self?.managerOperations = [:]
                    self?.verifyingManagerIds = []
                    self?.packageKegPolicyOverrides = [:]
                    self?.packageManagerPreferencesByFamilyKey = [:]
                    self?.homebrewKegAutoCleanupEnabled = false
                    self?.searchText = ""
                    self?.isRefreshing = false
                    self?.onboardingDetectionInProgress = false
                    self?.pinActionPackageIds = []
                    self?.upgradeActionPackageIds = []
                    self?.installActionPackageIds = []
                    self?.uninstallActionPackageIds = []
                    self?.packageDescriptionLoadingIds = []
                    self?.packageDescriptionUnavailableIds = []
                    self?.packageDescriptionSummaryByKey = [:]
                    self?.rustupToolchainDetailsByKey = [:]
                    self?.rustupToolchainDetailLoadingKeys = []
                    self?.rustupToolchainDetailUnavailableKeys = []
                    self?.rustupToolchainActionInFlightKeys = []
                    self?.upgradeActionTaskByPackage = [:]
                    self?.installActionTaskByPackage = [:]
                    self?.installActionNormalizedNameByPackageId = [:]
                    self?.uninstallActionTaskByPackage = [:]
                    self?.rustupToolchainActionTaskByKey = [:]
                    self?.rustupToolchainActionPackageByKey = [:]
                    self?.rustupToolchainActionSubmittedAtByKey = [:]
                    self?.descriptionLookupTaskIdsByPackage = [:]
                    self?.descriptionLookupStartedAtByPackage = [:]
                    self?.descriptionLookupPackageById = [:]
                    self?.activeRemoteSearchTaskIds = []
                    self?.managerActionTaskDescriptions = [:]
                    self?.managerActionTaskByManager = [:]
                    self?.managerActionTaskTypes = [:]
                    self?.managerActionTaskSubmittedAt = [:]
                    self?.managerVerificationAnchorTaskIdByManager = [:]
                    self?.managerVerificationStartedAtByManager = [:]
                    self?.managerPostInstallSetupHandledTaskIds = [:]
                    self?.localManagerActionTasks = [:]
                    self?.localManagerActionTaskCreatedAt = [:]
                    self?.lastObservedTaskId = 0
                    self?.onboardingDetectionAnchorTaskId = 0
                    self?.onboardingDetectionPendingManagers = []
                    self?.onboardingDetectionStartedAt = nil
                    self?.lastRefreshTrigger = nil
                    self?.lastCompletedRefreshAt = nil
                    self?.lastTaskSnapshotRefreshAt = .distantPast
                    UserDefaults.standard.removeObject(forKey: Self.onboardingCompletedKey)
                    UserDefaults.standard.removeObject(forKey: Self.acceptedLicenseTermsVersionKey)
                    UserDefaults.standard.removeObject(forKey: Self.acceptedLicenseTermsAcceptedAtUnixKey)
                    self?.hasCompletedOnboarding = false
                    self?.acceptedLicenseTermsVersion = nil
                    self?.acceptedLicenseTermsAcceptedAtUnix = nil
                }
                // Resume polling after reset
                self?.startPolling()
                self?.scheduleDerivedViewStateRefresh()
                completion(success)
            }
        }
    }

    /// Posts a VoiceOver announcement for state changes.
    func postAccessibilityAnnouncement(_ message: String) {
        NSAccessibility.post(
            element: NSApp as Any,
            notification: .announcementRequested,
            userInfo: [
                .announcement: message,
                .priority: NSAccessibilityPriorityLevel.high.rawValue
            ]
        )
    }

    func pruneOnboardingDetectionForDisabledManagers() {
        guard onboardingDetectionInProgress else { return }
        for (managerId, status) in managerStatuses where !status.enabled {
            onboardingDetectionPendingManagers.remove(managerId)
        }
        if onboardingDetectionPendingManagers.isEmpty {
            completeOnboardingDetectionProgress()
        }
    }

    func completeOnboardingDetectionProgress() {
        onboardingDetectionInProgress = false
        onboardingDetectionStatusSyncRequested = false
        onboardingDetectionPendingManagers.removeAll()
        onboardingDetectionStartedAt = nil
    }

    private func scheduleDerivedViewStateRefresh() {
        if !Thread.isMainThread {
            DispatchQueue.main.async { [weak self] in
                self?.scheduleDerivedViewStateRefresh()
            }
            return
        }

        derivedViewStateRefreshWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            self?.refreshDerivedViewStates()
        }
        derivedViewStateRefreshWorkItem = workItem
        DispatchQueue.main.async(execute: workItem)
    }

    private func refreshDerivedViewStates() {
        let installedCountByManager = Dictionary(grouping: installedPackages, by: \.managerId)
            .mapValues(\.count)
        let outdatedCountByManager = Dictionary(grouping: outdatedPackages, by: \.managerId)
            .mapValues(\.count)

        let visibleManagers = ManagerInfo.all.filter { manager in
            let status = managerStatuses[manager.id]
            let isImplemented = status?.isImplemented ?? manager.isImplemented
            let isEnabled = status?.enabled ?? true
            let hasPackageStateIssue = !(status?.packageStateIssues?.isEmpty ?? true)
            let isDetected = isManagerDetected(manager.id)
            return isImplemented && isDetected && (isEnabled || hasPackageStateIssue)
        }

        let failedManagerIds = Set(
            activeTasks
                .filter { $0.status.lowercased() == "failed" }
                .compactMap(\.managerId)
        )
        let runningManagerIds = Set(
            activeTasks
                .filter(\.isRunning)
                .compactMap(\.managerId)
        )
        let outdatedManagerIds = Set(outdatedPackages.map(\.managerId))

        var managerHealthById: [String: OperationalHealth] = [:]
        managerHealthById.reserveCapacity(visibleManagers.count)
        for manager in visibleManagers {
            let multiInstanceAttentionNeeded =
                managerStatuses[manager.id]?.multiInstanceState == "attention_needed"
            let hasPackageStateIssue =
                !(managerStatuses[manager.id]?.packageStateIssues?.isEmpty ?? true)
            if failedManagerIds.contains(manager.id) {
                managerHealthById[manager.id] = .error
            } else if runningManagerIds.contains(manager.id) {
                managerHealthById[manager.id] = .running
            } else if hasPackageStateIssue || multiInstanceAttentionNeeded {
                managerHealthById[manager.id] = .needsReview
            } else if outdatedManagerIds.contains(manager.id) {
                managerHealthById[manager.id] = .updatesReady
            } else {
                managerHealthById[manager.id] = .healthy
            }
        }

        let actionableFindingManagerIds = visibleManagers.compactMap { manager -> String? in
            let status = managerStatuses[manager.id]
            let requiresInstallDecision = status?.multiInstanceState == "attention_needed"
            let hasPackageStateIssue = !(status?.packageStateIssues?.isEmpty ?? true)
            return requiresInstallDecision || hasPackageStateIssue ? manager.id : nil
        }
        let failedTasks = activeTasks.filter { $0.status.lowercased() == "failed" }
        let interruptedTasks = activeTasks.filter { $0.status.lowercased() == "interrupted" }
        let failedTaskCount = failedTasks.count
        let runningTasks = activeTasks.filter(\.isRunning)
        let runningTaskCount = runningTasks.count
        let implementedManagers = ManagerInfo.all.filter { manager in
            managerStatuses[manager.id]?.isImplemented ?? manager.isImplemented
        }
        let detectedManagerCount = implementedManagers.filter { manager in
            isManagerDetected(manager.id)
        }.count
        let taskManagerByID = Dictionary(
            uniqueKeysWithValues: activeTasks.compactMap { task in
                task.managerId.map { (task.id, $0) }
            }
        )
        let approvalManagerIDs = taskTimeoutPrompts.compactMap { prompt in
            taskManagerByID[String(prompt.taskId)]
        }
        let wayfinderRelatedManagerIDs: [String]
        if !approvalManagerIDs.isEmpty {
            wayfinderRelatedManagerIDs = approvalManagerIDs
        } else if !failedTasks.isEmpty || !interruptedTasks.isEmpty {
            wayfinderRelatedManagerIDs = (failedTasks + interruptedTasks).compactMap(\.managerId)
        } else if !runningTasks.isEmpty {
            wayfinderRelatedManagerIDs = runningTasks.compactMap(\.managerId)
        } else if !actionableFindingManagerIds.isEmpty {
            wayfinderRelatedManagerIDs = actionableFindingManagerIds
        } else if !outdatedPackages.isEmpty {
            wayfinderRelatedManagerIDs = Array(outdatedManagerIds).sorted()
        } else {
            wayfinderRelatedManagerIDs = []
        }
        let affectedRouteStages = Set(
            wayfinderRelatedManagerIDs.compactMap { managerID in
                ManagerInfo.find(byId: managerID).flatMap { manager in
                    WayfinderPopoverRouteStage.stage(forManagerCategory: manager.category)
                }
            }
        )
        let wayfinderRelatedRouteStages = WayfinderPopoverRouteStage.allCases.filter(
            affectedRouteStages.contains
        )
        let wayfinderRelatedManagerIDsByStage = wayfinderRelatedManagerIDs.reduce(
            into: [WayfinderPopoverRouteStage: String]()
        ) { result, managerID in
            guard let manager = ManagerInfo.find(byId: managerID),
                  let stage = WayfinderPopoverRouteStage.stage(
                      forManagerCategory: manager.category
                  ),
                  result[stage] == nil else {
                return
            }
            result[stage] = managerID
        }
        let wayfinderFindingContext = actionableFindingManagerIds.first.flatMap {
            makeWayfinderPopoverFindingContext(for: $0)
        }
        let wayfinderInput = WayfinderProjectionInput(
            serviceAvailable: isConnected,
            networkAvailable: networkAvailability != .unavailable,
            approvalTaskIDs: taskTimeoutPrompts.map { String($0.taskId) },
            failedTaskIDs: failedTasks.map(\.id),
            interruptedTaskIDs: interruptedTasks.map(\.id),
            activeTaskIDs: runningTasks.map(\.id),
            actionableFindingIDs: actionableFindingManagerIds,
            updateCount: outdatedPackages.count,
            isRefreshing: isRefreshing,
            freshnessDate: lastCompletedRefreshAt,
            coverage: WayfinderCoverage(
                completed: detectedManagerCount,
                total: implementedManagers.count
            )
        )

        let environmentBriefInput = makeEnvironmentBriefInput(
            intendedManagers: implementedManagers
        )

        overviewState.apply(
            wayfinderInput: wayfinderInput,
            environmentBriefInput: environmentBriefInput,
            failedTaskCount: failedTaskCount,
            runningTaskCount: runningTaskCount,
            outdatedPackagesCount: outdatedPackages.count,
            isRefreshing: isRefreshing,
            visibleManagers: visibleManagers,
            wayfinderPopoverState: WayfinderPopoverDerivedState(
                relatedRouteStages: wayfinderRelatedRouteStages,
                relatedManagerIDsByStage: wayfinderRelatedManagerIDsByStage,
                detectedManagerCount: detectedManagerCount,
                findingContext: wayfinderFindingContext
            ),
            outdatedCountByManager: outdatedCountByManager,
            managerHealthById: managerHealthById,
            recentTasksTop10: Array(activeTasks.prefix(10))
        )
        if hasCompletedOnboarding && !requiresLicenseTermsAcceptance {
            firstRunPresentationModel.clear()
        } else {
            firstRunPresentationModel.synchronize(
                currentBrief: overviewState.environmentBrief,
                requiresLicenseAcceptance: requiresLicenseTermsAcceptance
            )
        }

        managersState.apply(
            authoritativeManagers: sortedManagersByPriority(
                implementedManagers.filter { $0.authority == .authoritative }
            ),
            standardManagers: sortedManagersByPriority(
                implementedManagers.filter { $0.authority == .standard }
            ),
            guardedManagers: sortedManagersByPriority(
                implementedManagers.filter { $0.authority == .guarded }
            ),
            managerStatusesById: managerStatuses,
            managerOperationsById: managerOperations,
            installedCountByManager: installedCountByManager,
            outdatedCountByManager: outdatedCountByManager
        )
    }

    private func makeWayfinderPopoverFindingContext(
        for managerID: String
    ) -> WayfinderPopoverFindingContext? {
        guard let manager = ManagerInfo.find(byId: managerID),
              let status = managerStatuses[managerID] else {
            return nil
        }

        if status.multiInstanceState == "attention_needed" {
            return WayfinderPopoverFindingContext(
                title: WayfinderLocalizedText(
                    key: "app.popover.wayfinder.context.manager_needs_decision",
                    arguments: ["manager": manager.displayName]
                ),
                detail: WayfinderLocalizedText(
                    key: L10n.App.Inspector.MultiInstance.attentionTitle
                )
            )
        }

        guard let issue = status.packageStateIssues?.first else { return nil }
        let detail: WayfinderLocalizedText
        switch issue.issueCode {
        case "post_install_setup_required":
            detail = WayfinderLocalizedText(
                key: "app.inspector.package_state_issue.setup_required.title"
            )
        case "metadata_only_install":
            detail = WayfinderLocalizedText(
                key: L10n.App.Managers.State.metadataMismatch,
                arguments: ["package": issue.packageName]
            )
        case "homebrew_cellar_lock_conflict":
            detail = WayfinderLocalizedText(
                key: "app.inspector.package_state_issue.homebrew_lock.title"
            )
        default:
            detail = WayfinderLocalizedText(
                key: "app.popover.wayfinder.context.package_state_needs_review"
            )
        }

        return WayfinderPopoverFindingContext(
            title: WayfinderLocalizedText(
                key: "app.popover.wayfinder.context.manager_needs_review",
                arguments: ["manager": manager.displayName]
            ),
            detail: detail
        )
    }

    func renderedPackageDescription(
        for package: PackageItem,
        summaryOverride: String? = nil
    ) -> PackageDescriptionRenderer.RenderedDescription? {
        let summaryText = summaryOverride ?? package.summary
        let key = [
            package.id,
            package.version,
            package.latestVersion ?? "",
            summaryText ?? ""
        ].joined(separator: "|")

        if let cached = packageDescriptionRenderCache[key] {
            touchDescriptionRenderCacheKey(key)
            switch cached {
            case .none:
                return nil
            case .rendered(let rendered):
                return rendered
            }
        }

        let rendered = PackageDescriptionRenderer.render(summaryText)
        packageDescriptionRenderCache[key] =
            rendered.map(PackageDescriptionRenderCacheEntry.rendered)
            ?? PackageDescriptionRenderCacheEntry.none
        touchDescriptionRenderCacheKey(key)
        trimPackageDescriptionRenderCacheIfNeeded()
        return rendered
    }

    private func touchDescriptionRenderCacheKey(_ key: String) {
        packageDescriptionRenderCacheOrder.removeAll { $0 == key }
        packageDescriptionRenderCacheOrder.append(key)
    }

    private func trimPackageDescriptionRenderCacheIfNeeded() {
        while packageDescriptionRenderCacheOrder.count > Self.maxPackageDescriptionRenderCacheEntries {
            let oldest = packageDescriptionRenderCacheOrder.removeFirst()
            packageDescriptionRenderCache.removeValue(forKey: oldest)
        }
    }
}
