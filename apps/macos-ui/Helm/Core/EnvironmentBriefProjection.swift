import Foundation

enum EnvironmentBriefArchitecture: String, Codable, Equatable {
    case arm64
    case x86_64
}

enum EnvironmentBriefEligibility: String, Codable, Equatable {
    case eligible
    case ineligible
    case unknown
}

enum EnvironmentBriefManagementState: String, Codable, Equatable {
    case ready
    case setupRequired = "setup_required"
    case multipleInstancesAttention = "multiple_instances_attention"
    case detectedUnmanageable = "detected_unmanageable"
    case notInstalled = "not_installed"
    case unknown
}

enum EnvironmentBriefProvenance: String, Codable, Equatable {
    case unknown
    case system
    case homebrew
    case macports
    case nix
    case asdf
    case mise
    case rustupInit = "rustup_init"
    case enterpriseManaged = "enterprise_managed"
    case sourceBuild = "source_build"
}

enum EnvironmentBriefFreshness: String, Codable, Equatable {
    case current
    case cached
    case unknown
}

enum EnvironmentBriefObservationClass: String, Codable, Equatable {
    case localOnly = "local_only"
    case localAndDisclosedNetwork = "local_and_disclosed_network"
}

struct EnvironmentBriefSystem: Codable, Equatable {
    let osVersion: String
    let architecture: EnvironmentBriefArchitecture
    let activeShell: String
    let distributionChannel: String
    let updateAuthority: String

    static func current(
        distributionChannel: String,
        updateAuthority: String,
        processInfo: ProcessInfo = .processInfo
    ) -> EnvironmentBriefSystem {
        let version = processInfo.operatingSystemVersion
        let shellPath = processInfo.environment["SHELL"] ?? "/bin/zsh"
        let shell = URL(fileURLWithPath: shellPath).lastPathComponent

        #if arch(arm64)
        let architecture = EnvironmentBriefArchitecture.arm64
        #else
        let architecture = EnvironmentBriefArchitecture.x86_64
        #endif

        return EnvironmentBriefSystem(
            osVersion: "\(version.majorVersion).\(version.minorVersion).\(version.patchVersion)",
            architecture: architecture,
            activeShell: shell.isEmpty ? "zsh" : shell,
            distributionChannel: distributionChannel,
            updateAuthority: updateAuthority
        )
    }
}

struct EnvironmentBriefManagerObservation: Codable, Equatable {
    let manager: String
    let detected: Bool
    let eligibility: EnvironmentBriefEligibility
    let managementState: EnvironmentBriefManagementState
    let activeInstallationMethod: String?
    let provenance: EnvironmentBriefProvenance?
    let freshness: EnvironmentBriefFreshness
}

struct EnvironmentBriefCoverage: Codable, Equatable {
    let intendedManagerCount: Int
    let currentManagerCount: Int
    let cachedManagerCount: Int
    let failedManagers: [String]
    let cancelledManagers: [String]
    let deferredManagers: [String]
}

struct EnvironmentBrief: Codable, Equatable {
    static let currentSchemaVersion = "1.0.0"

    let schemaVersion: String
    let briefID: UUID
    let revision: UInt64
    let generatedAt: String
    let system: EnvironmentBriefSystem
    let discoveredManagers: [EnvironmentBriefManagerObservation]
    let coverage: EnvironmentBriefCoverage
    let observationClass: EnvironmentBriefObservationClass

    private enum CodingKeys: String, CodingKey {
        case schemaVersion
        case briefID = "briefId"
        case revision
        case generatedAt
        case system
        case discoveredManagers
        case coverage
        case observationClass
    }
}

struct EnvironmentBriefProjectionInput: Equatable {
    let system: EnvironmentBriefSystem
    let intendedManagerIDs: [String]
    let observations: [EnvironmentBriefManagerObservation]
    let failedManagerIDs: [String]
    let cancelledManagerIDs: [String]
    let deferredManagerIDs: [String]
    let observationClass: EnvironmentBriefObservationClass
}

enum EnvironmentBriefProjector {
    static func project(
        _ input: EnvironmentBriefProjectionInput,
        replacing previous: EnvironmentBrief? = nil,
        generatedAt: Date = Date(),
        briefID: UUID = UUID()
    ) -> EnvironmentBrief {
        let observations = canonicalObservations(input.observations)
        let coverage = EnvironmentBriefCoverage(
            intendedManagerCount: canonicalIDs(input.intendedManagerIDs).count,
            currentManagerCount: observations.filter { $0.freshness == .current }.count,
            cachedManagerCount: observations.filter { $0.freshness == .cached }.count,
            failedManagers: canonicalIDs(input.failedManagerIDs),
            cancelledManagers: canonicalIDs(input.cancelledManagerIDs),
            deferredManagers: canonicalIDs(input.deferredManagerIDs)
        )

        if let previous,
           previous.system == input.system,
           previous.discoveredManagers == observations,
           previous.coverage == coverage,
           previous.observationClass == input.observationClass {
            return previous
        }

        return EnvironmentBrief(
            schemaVersion: EnvironmentBrief.currentSchemaVersion,
            briefID: previous?.briefID ?? briefID,
            revision: nextRevision(after: previous?.revision),
            generatedAt: ISO8601DateFormatter().string(from: generatedAt),
            system: input.system,
            discoveredManagers: observations,
            coverage: coverage,
            observationClass: input.observationClass
        )
    }

    private static func canonicalObservations(
        _ observations: [EnvironmentBriefManagerObservation]
    ) -> [EnvironmentBriefManagerObservation] {
        var byManager: [String: EnvironmentBriefManagerObservation] = [:]
        for observation in observations.sorted(by: { $0.manager < $1.manager }) {
            let manager = observation.manager.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !manager.isEmpty else { continue }
            byManager[manager] = EnvironmentBriefManagerObservation(
                manager: manager,
                detected: observation.detected,
                eligibility: observation.eligibility,
                managementState: observation.detected ? observation.managementState : .notInstalled,
                activeInstallationMethod: nonEmpty(observation.activeInstallationMethod),
                provenance: observation.detected ? (observation.provenance ?? .unknown) : nil,
                freshness: observation.detected ? observation.freshness : .unknown
            )
        }
        return byManager.values.sorted { $0.manager < $1.manager }
    }

    private static func canonicalIDs(_ values: [String]) -> [String] {
        Array(Set(values.compactMap(nonEmpty))).sorted()
    }

    private static func nonEmpty(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private static func nextRevision(after revision: UInt64?) -> UInt64 {
        guard let revision else { return 1 }
        return revision == .max ? .max : revision + 1
    }
}
