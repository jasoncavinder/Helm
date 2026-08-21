import Combine
import Foundation

enum AppNotificationPreference {
    static func resolvedEnabled(storedValue: Any?) -> Bool {
        (storedValue as? NSNumber)?.boolValue ?? true
    }
}

struct AppUpdateNotificationEvaluation: Equatable {
    let observedFingerprint: String?
    let shouldNotify: Bool
    let updatesReadySuppressedForExecution: Bool
}

struct AppUpdateNotificationSnapshot: Equatable {
    let updateIdentifiers: [String]
    let updateCount: Int
    let automaticUpdateCount: Int

    init(
        updateIdentifiers: [String],
        updateCount: Int,
        automaticUpdateCount: Int
    ) {
        self.updateIdentifiers = updateIdentifiers
        self.updateCount = updateCount
        self.automaticUpdateCount = automaticUpdateCount
    }

    init(packages: [PackageItem], automaticUpdateCount: Int) {
        var updateIdentifiers = packages.map { package in
            [
                package.managerId,
                package.id,
                PackageIdentity.normalizedKnownVersion(package.version)
                    ?? PackageVersionStorage.unknown,
                package.latestVersion ?? "",
            ].joined(separator: "|")
        }
        if !packages.isEmpty {
            updateIdentifiers.append("upgrade-all-available:\(automaticUpdateCount > 0)")
        }
        self.updateIdentifiers = updateIdentifiers
        self.updateCount = packages.count
        self.automaticUpdateCount = automaticUpdateCount
    }
}

struct AppUpdateNotificationSnapshotEvent: Equatable {
    let revision: UInt64
    let snapshot: AppUpdateNotificationSnapshot
}

struct OutdatedPackageSnapshotRevisionState: Equatable {
    private(set) var latestIssuedRevision: UInt64 = 0
    private(set) var latestAppliedRevision: UInt64 = 0

    mutating func issueRequest() -> UInt64 {
        latestIssuedRevision &+= 1
        return latestIssuedRevision
    }

    mutating func acceptResponse(revision: UInt64) -> Bool {
        guard revision > latestAppliedRevision else { return false }
        latestAppliedRevision = revision
        return true
    }
}

final class AppUpdateNotificationEventTracker {
    let outdatedPackagesSnapshotPublisher = PassthroughSubject<
        AppUpdateNotificationSnapshotEvent,
        Never
    >()
    let helmOnlyPlanStartedPublisher = PassthroughSubject<
        AppUpdateNotificationSnapshot,
        Never
    >()

    private var outdatedPackagesSnapshotRevisionState = OutdatedPackageSnapshotRevisionState()

    func issueOutdatedPackagesSnapshotRequest() -> UInt64 {
        outdatedPackagesSnapshotRevisionState.issueRequest()
    }

    func acceptOutdatedPackagesSnapshotResponse(revision: UInt64) -> Bool {
        outdatedPackagesSnapshotRevisionState.acceptResponse(revision: revision)
    }

    func publishOutdatedPackagesSnapshot(
        revision: UInt64,
        snapshot: AppUpdateNotificationSnapshot
    ) {
        outdatedPackagesSnapshotPublisher.send(
            AppUpdateNotificationSnapshotEvent(revision: revision, snapshot: snapshot)
        )
    }

    func publishHelmOnlyPlanStarted(snapshot: AppUpdateNotificationSnapshot) {
        helmOnlyPlanStartedPublisher.send(snapshot)
    }
}

enum AppUpdateNotificationObservation: Equatable {
    case availabilityChanged
    case backendExecutionStarted
    case backendExecutionEnded(awaitingSnapshotRevision: UInt64)
    case postExecutionSnapshotPublished(revision: UInt64)
    case helmOnlyPlanStarted
}

struct AppUpdateNotificationState: Equatable {
    private(set) var observedFingerprint: String?
    private(set) var backendExecutionInProgress = false
    private(set) var requiredPostExecutionSnapshotRevision: UInt64?

    var isExecutionSuppressed: Bool {
        backendExecutionInProgress || requiredPostExecutionSnapshotRevision != nil
    }

    mutating func evaluate(
        updateIdentifiers: [String],
        notificationsEnabled: Bool,
        interactiveSurfaceVisible: Bool,
        observation: AppUpdateNotificationObservation
    ) -> AppUpdateNotificationEvaluation {
        let suppressForObservation: Bool
        switch observation {
        case .availabilityChanged:
            suppressForObservation = isExecutionSuppressed
        case .backendExecutionStarted:
            backendExecutionInProgress = true
            suppressForObservation = true
        case let .backendExecutionEnded(awaitingSnapshotRevision):
            backendExecutionInProgress = false
            requiredPostExecutionSnapshotRevision = max(
                requiredPostExecutionSnapshotRevision ?? 0,
                awaitingSnapshotRevision
            )
            suppressForObservation = true
        case let .postExecutionSnapshotPublished(revision):
            suppressForObservation = isExecutionSuppressed
            if let requiredPostExecutionSnapshotRevision,
               revision >= requiredPostExecutionSnapshotRevision {
                self.requiredPostExecutionSnapshotRevision = nil
            }
        case .helmOnlyPlanStarted:
            // Sparkle remains the interactive execution authority, so this event consumes the
            // reviewed Plan snapshot without conflating unrelated manual update checks.
            suppressForObservation = true
        }

        let evaluation = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: updateIdentifiers,
            previousFingerprint: observedFingerprint,
            notificationsEnabled: notificationsEnabled,
            interactiveSurfaceVisible: interactiveSurfaceVisible,
            updatesReadySuppressedForExecution: suppressForObservation
        )
        observedFingerprint = evaluation.observedFingerprint
        return evaluation
    }
}

enum AppUpdateNotificationPolicy {
    static func evaluate(
        updateIdentifiers: [String],
        previousFingerprint: String?,
        notificationsEnabled: Bool,
        interactiveSurfaceVisible: Bool,
        updatesReadySuppressedForExecution: Bool
    ) -> AppUpdateNotificationEvaluation {
        let fingerprint = normalizedFingerprint(updateIdentifiers)
        guard fingerprint != previousFingerprint else {
            return AppUpdateNotificationEvaluation(
                observedFingerprint: previousFingerprint,
                shouldNotify: false,
                updatesReadySuppressedForExecution: updatesReadySuppressedForExecution
            )
        }

        return AppUpdateNotificationEvaluation(
            observedFingerprint: fingerprint,
            shouldNotify: fingerprint != nil
                && notificationsEnabled
                && !interactiveSurfaceVisible
                && !updatesReadySuppressedForExecution,
            updatesReadySuppressedForExecution: updatesReadySuppressedForExecution
        )
    }

    private static func normalizedFingerprint(_ identifiers: [String]) -> String? {
        let normalized = Set(
            identifiers.compactMap { identifier -> String? in
                let trimmed = identifier.trimmingCharacters(in: .whitespacesAndNewlines)
                return trimmed.isEmpty ? nil : trimmed
            }
        )
        .sorted()

        return normalized.isEmpty ? nil : normalized.joined(separator: "\u{1F}")
    }
}
