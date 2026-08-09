import Foundation
import Combine

enum EnvironmentBriefFixtureName: String, Codable, CaseIterable {
    case firstUseful = "first-useful"
    case current
    case partial
    case offline
    case serviceFailure = "service-failure"
}

struct EnvironmentBriefPresentationFixture: Codable, Equatable {
    static let currentSchemaVersion = "1.0.0"

    let schemaVersion: String
    let name: EnvironmentBriefFixtureName
    let brief: EnvironmentBrief
}

enum EnvironmentBriefFixtureProvider {
    static let environmentKey = "HELM_ENVIRONMENT_BRIEF_FIXTURE"

    static func active(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> EnvironmentBriefPresentationFixture? {
        #if DEBUG
        guard let rawName = environment[environmentKey],
              let name = EnvironmentBriefFixtureName(rawValue: rawName) else {
            return nil
        }
        return fixture(named: name)
        #else
        return nil
        #endif
    }

    static func fixture(named name: EnvironmentBriefFixtureName) -> EnvironmentBriefPresentationFixture {
        let input: EnvironmentBriefProjectionInput
        switch name {
        case .firstUseful:
            input = makeInput(
                intendedManagerIDs: ["homebrew_formula", "mise", "rustup"],
                observations: [observation("mise", provenance: .mise)]
            )
        case .current:
            input = makeInput(
                intendedManagerIDs: ["homebrew_formula", "mise", "rustup"],
                observations: [
                    observation("homebrew_formula", provenance: .homebrew),
                    observation("mise", provenance: .mise),
                    observation("rustup", provenance: .rustupInit)
                ]
            )
        case .partial:
            input = makeInput(
                intendedManagerIDs: ["homebrew_formula", "macports", "mise", "rustup"],
                observations: [
                    observation("homebrew_formula", provenance: .homebrew),
                    observation("mise", provenance: .mise, freshness: .cached)
                ],
                failedManagerIDs: ["macports"],
                deferredManagerIDs: ["rustup"]
            )
        case .offline:
            input = makeInput(
                intendedManagerIDs: ["homebrew_formula", "mise"],
                observations: [
                    observation("homebrew_formula", provenance: .homebrew, freshness: .cached),
                    observation("mise", provenance: .mise, freshness: .cached)
                ]
            )
        case .serviceFailure:
            input = makeInput(
                intendedManagerIDs: ["homebrew_formula", "mise", "rustup"],
                observations: [],
                failedManagerIDs: ["homebrew_formula", "mise", "rustup"]
            )
        }

        return EnvironmentBriefPresentationFixture(
            schemaVersion: EnvironmentBriefPresentationFixture.currentSchemaVersion,
            name: name,
            brief: EnvironmentBriefProjector.project(
                input,
                generatedAt: Date(timeIntervalSince1970: 1_775_260_800),
                briefID: fixtureID(for: name)
            )
        )
    }

    private static func makeInput(
        intendedManagerIDs: [String],
        observations: [EnvironmentBriefManagerObservation],
        failedManagerIDs: [String] = [],
        deferredManagerIDs: [String] = []
    ) -> EnvironmentBriefProjectionInput {
        EnvironmentBriefProjectionInput(
            system: EnvironmentBriefSystem(
                osVersion: "26.6.0",
                architecture: .arm64,
                activeShell: "zsh",
                distributionChannel: "developer_id",
                updateAuthority: "sparkle"
            ),
            intendedManagerIDs: intendedManagerIDs,
            observations: observations,
            failedManagerIDs: failedManagerIDs,
            cancelledManagerIDs: [],
            deferredManagerIDs: deferredManagerIDs,
            observationClass: .localOnly
        )
    }

    private static func observation(
        _ manager: String,
        provenance: EnvironmentBriefProvenance,
        freshness: EnvironmentBriefFreshness = .current
    ) -> EnvironmentBriefManagerObservation {
        EnvironmentBriefManagerObservation(
            manager: manager,
            detected: true,
            eligibility: .eligible,
            managementState: .ready,
            activeInstallationMethod: nil,
            provenance: provenance,
            freshness: freshness
        )
    }

    private static func fixtureID(for name: EnvironmentBriefFixtureName) -> UUID {
        let finalByte: UInt8
        switch name {
        case .firstUseful: finalByte = 1
        case .current: finalByte = 2
        case .partial: finalByte = 3
        case .offline: finalByte = 4
        case .serviceFailure: finalByte = 5
        }
        return UUID(uuid: (
            0x55, 0x0e, 0x84, 0x00,
            0xe2, 0x9b,
            0x41, 0xd4,
            0xa7, 0x16,
            0x44, 0x66, 0x55, 0x44, 0x00, finalByte
        ))
    }
}

enum EnvironmentBriefFirstRunMode: String, Equatable {
    case disabled
    case enabled
    case preview
}

enum EnvironmentBriefPreviewAppearance: String, Equatable {
    case system
    case light
    case dark
}

enum EnvironmentBriefFirstRunConfiguration {
    static let environmentKey = "HELM_ENVIRONMENT_BRIEF_FIRST_RUN"
    static let appearanceEnvironmentKey = "HELM_ENVIRONMENT_BRIEF_APPEARANCE"

    static func mode(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> EnvironmentBriefFirstRunMode {
        #if DEBUG
        let rawMode = environment[environmentKey]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        switch rawMode {
        case "1", "true", "on", "enabled":
            return .enabled
        case "preview":
            return .preview
        default:
            return .disabled
        }
        #else
        return .disabled
        #endif
    }

    static func shouldPresent(
        mode: EnvironmentBriefFirstRunMode,
        hasCompletedOnboarding: Bool,
        dismissedPreview: Bool
    ) -> Bool {
        switch mode {
        case .disabled:
            return false
        case .enabled:
            return !hasCompletedOnboarding
        case .preview:
            return !dismissedPreview
        }
    }

    static func previewAppearance(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> EnvironmentBriefPreviewAppearance {
        #if DEBUG
        guard let rawAppearance = environment[appearanceEnvironmentKey]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased() else {
            return .system
        }
        return EnvironmentBriefPreviewAppearance(rawValue: rawAppearance) ?? .system
        #else
        return .system
        #endif
    }
}

enum EnvironmentBriefPresentationKind: Equatable {
    case mapping
    case current
    case cached
    case partial
    case serviceFailure
}

struct EnvironmentBriefPresentationSummary: Equatable {
    let kind: EnvironmentBriefPresentationKind
    let intendedManagerCount: Int
    let mappedManagerCount: Int
    let readyManagerCount: Int
    let attentionCount: Int
    let completionFraction: Double

    static func make(from brief: EnvironmentBrief) -> EnvironmentBriefPresentationSummary {
        let coverage = brief.coverage
        let mappedManagerIDs = Set(
            brief.discoveredManagers.compactMap { observation in
                observation.freshness == .unknown ? nil : observation.manager
            }
        )
        let terminalManagerIDs = mappedManagerIDs
            .union(coverage.failedManagers)
            .union(coverage.cancelledManagers)
            .union(coverage.deferredManagers)
        let attentionManagerIDs = Set(
            brief.discoveredManagers.compactMap { observation in
                observation.eligibility != .eligible || observation.managementState != .ready
                    ? observation.manager
                    : nil
            }
        )
        .union(coverage.failedManagers)
        .union(coverage.cancelledManagers)
        let mappedCount = mappedManagerIDs.count
        let terminalCount = terminalManagerIDs.count
        let attentionCount = attentionManagerIDs.count

        let kind: EnvironmentBriefPresentationKind
        if coverage.intendedManagerCount > 0,
           mappedCount == 0,
           coverage.failedManagers.count >= coverage.intendedManagerCount {
            kind = .serviceFailure
        } else if !coverage.failedManagers.isEmpty
            || !coverage.cancelledManagers.isEmpty
            || !coverage.deferredManagers.isEmpty {
            kind = .partial
        } else if coverage.currentManagerCount == 0 && coverage.cachedManagerCount > 0 {
            kind = .cached
        } else if terminalCount < coverage.intendedManagerCount {
            kind = .mapping
        } else {
            kind = .current
        }

        let completionFraction: Double
        if coverage.intendedManagerCount == 0 {
            completionFraction = kind == .current ? 1 : 0
        } else {
            completionFraction = min(
                Double(terminalCount) / Double(coverage.intendedManagerCount),
                1
            )
        }

        return EnvironmentBriefPresentationSummary(
            kind: kind,
            intendedManagerCount: coverage.intendedManagerCount,
            mappedManagerCount: mappedCount,
            readyManagerCount: brief.discoveredManagers.filter { observation in
                observation.detected
                    && observation.eligibility == .eligible
                    && observation.managementState == .ready
            }.count,
            attentionCount: attentionCount,
            completionFraction: completionFraction
        )
    }
}

enum FirstRunPresentationStage: String, Codable, Equatable {
    case legal
    case discovering
    case brief
}

struct FirstRunPresentationState: Codable, Equatable {
    static let currentSchemaVersion = "1.0.0"

    let schemaVersion: String
    let stage: FirstRunPresentationStage
    let briefID: UUID?
    let briefRevision: UInt64?
    let selectedManagerID: String?

    private enum CodingKeys: String, CodingKey {
        case schemaVersion
        case stage
        case briefID = "briefId"
        case briefRevision
        case selectedManagerID = "selectedManagerId"
    }
}

enum FirstRunPresentationRestorer {
    static func restore(
        saved: FirstRunPresentationState?,
        currentBrief: EnvironmentBrief?,
        requiresLicenseAcceptance: Bool
    ) -> FirstRunPresentationState {
        if requiresLicenseAcceptance {
            return state(stage: .legal, brief: currentBrief, selectedManagerID: nil)
        }
        guard let currentBrief else {
            return state(stage: .discovering, brief: nil, selectedManagerID: nil)
        }

        let savedSelection = saved?.stage == .brief ? saved?.selectedManagerID : nil
        let currentManagerIDs = Set(currentBrief.discoveredManagers.map(\.manager))
        let restoredSelection = savedSelection.flatMap { managerID in
            currentManagerIDs.contains(managerID) ? managerID : nil
        }
        return state(stage: .brief, brief: currentBrief, selectedManagerID: restoredSelection)
    }

    private static func state(
        stage: FirstRunPresentationStage,
        brief: EnvironmentBrief?,
        selectedManagerID: String?
    ) -> FirstRunPresentationState {
        FirstRunPresentationState(
            schemaVersion: FirstRunPresentationState.currentSchemaVersion,
            stage: stage,
            briefID: brief?.briefID,
            briefRevision: brief?.revision,
            selectedManagerID: selectedManagerID
        )
    }
}

struct FirstRunPresentationStateStore {
    static let defaultKey = "projectWOW.firstRunPresentationState.v1"

    private let defaults: UserDefaults
    private let key: String

    init(defaults: UserDefaults = .standard, key: String = defaultKey) {
        self.defaults = defaults
        self.key = key
    }

    func load() -> FirstRunPresentationState? {
        guard let data = defaults.data(forKey: key),
              let state = try? JSONDecoder().decode(FirstRunPresentationState.self, from: data),
              state.schemaVersion == FirstRunPresentationState.currentSchemaVersion else {
            return nil
        }
        return state
    }

    func save(_ state: FirstRunPresentationState) {
        guard let data = try? JSONEncoder().encode(state) else { return }
        defaults.set(data, forKey: key)
    }

    func clear() {
        defaults.removeObject(forKey: key)
    }
}

final class FirstRunPresentationModel: ObservableObject {
    @Published private(set) var state: FirstRunPresentationState?

    private let store: FirstRunPresentationStateStore

    init(store: FirstRunPresentationStateStore = FirstRunPresentationStateStore()) {
        self.store = store
        self.state = store.load()
    }

    func synchronize(
        currentBrief: EnvironmentBrief?,
        requiresLicenseAcceptance: Bool
    ) {
        let restored = FirstRunPresentationRestorer.restore(
            saved: state,
            currentBrief: currentBrief,
            requiresLicenseAcceptance: requiresLicenseAcceptance
        )
        guard restored != state else { return }
        state = restored
        store.save(restored)
    }

    func clear() {
        guard state != nil else {
            store.clear()
            return
        }
        state = nil
        store.clear()
    }
}
