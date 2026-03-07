import Foundation

struct UpgradePreviewPlanner {
    struct Entry: Equatable {
        let manager: String
        let count: Int
    }

    struct PlanStep: Equatable {
        let id: String
        let orderIndex: UInt64
        let managerId: String
        let authority: String
        let packageName: String
        let reasonLabelKey: String
    }

    struct Candidate {
        let managerId: String
        let pinned: Bool
    }

    struct ProjectedTaskState {
        let taskId: UInt64
        let status: String
    }

    static let allManagersScopeId = "__all_managers__"

    static func count(
        candidates: [Candidate],
        managerEnabled: [String: Bool],
        includePinned: Bool,
        allowOsUpdates: Bool,
        safeModeEnabled: Bool
    ) -> Int {
        candidates.filter {
            includeForUpgradePreview(
                candidate: $0,
                managerEnabled: managerEnabled,
                includePinned: includePinned,
                allowOsUpdates: allowOsUpdates,
                safeModeEnabled: safeModeEnabled
            )
        }.count
    }

    static func breakdown(
        candidates: [Candidate],
        managerEnabled: [String: Bool],
        includePinned: Bool,
        allowOsUpdates: Bool,
        safeModeEnabled: Bool,
        managerName: (String) -> String
    ) -> [Entry] {
        var counts: [String: Int] = [:]

        for candidate in candidates {
            guard includeForUpgradePreview(
                candidate: candidate,
                managerEnabled: managerEnabled,
                includePinned: includePinned,
                allowOsUpdates: allowOsUpdates,
                safeModeEnabled: safeModeEnabled
            ) else {
                continue
            }
            let manager = managerName(candidate.managerId)
            counts[manager, default: 0] += 1
        }

        return counts
            .map { Entry(manager: $0.key, count: $0.value) }
            .sorted { lhs, rhs in
                if lhs.count == rhs.count {
                    return lhs.manager.localizedCaseInsensitiveCompare(rhs.manager) == .orderedAscending
                }
                return lhs.count > rhs.count
            }
    }

    static func authorityRank(for authority: String) -> Int {
        switch authority.lowercased() {
        case "authoritative":
            return 0
        case "standard":
            return 1
        case "guarded":
            return 2
        case "detection_only":
            return 3
        default:
            return 4
        }
    }

    static func sortedForExecution(_ steps: [PlanStep]) -> [PlanStep] {
        steps.sorted { lhs, rhs in
            let lhsRank = authorityRank(for: lhs.authority)
            let rhsRank = authorityRank(for: rhs.authority)
            if lhsRank != rhsRank { return lhsRank < rhsRank }
            if lhs.orderIndex != rhs.orderIndex { return lhs.orderIndex < rhs.orderIndex }
            if lhs.managerId != rhs.managerId { return lhs.managerId < rhs.managerId }
            return lhs.packageName < rhs.packageName
        }
    }

    static func scopedForExecution(
        from steps: [PlanStep],
        managerScopeId: String,
        packageFilter: String
    ) -> [PlanStep] {
        let trimmedFilter = packageFilter.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return sortedForExecution(steps).filter { step in
            let managerMatches = managerScopeId == allManagersScopeId || managerScopeId == step.managerId
            let packageMatches = trimmedFilter.isEmpty
                || step.packageName.lowercased().contains(trimmedFilter)
                || step.reasonLabelKey.lowercased().contains(trimmedFilter)
            return managerMatches && packageMatches
        }
    }

    static func shouldRunScopedStep(
        status: String,
        hasProjectedTask: Bool,
        managerId: String,
        safeModeEnabled: Bool
    ) -> Bool {
        let normalized = status.lowercased()
        if normalized == "completed" {
            return false
        }
        if isInFlightStatus(status: normalized, hasProjectedTask: hasProjectedTask) {
            return false
        }
        if managerId == "softwareupdate" && safeModeEnabled {
            return false
        }
        return true
    }

    static func isInFlightStatus(status: String, hasProjectedTask: Bool) -> Bool {
        let normalized = status.lowercased()
        if normalized == "running" {
            return true
        }
        if normalized == "queued" && hasProjectedTask {
            return true
        }
        return false
    }

    static func planStepId(managerId: String?, labelArgs: [String: String]?) -> String? {
        if let explicit = labelArgs?["plan_step_id"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !explicit.isEmpty {
            return explicit
        }
        if managerId == "softwareupdate" {
            return "softwareupdate:__confirm_os_updates__"
        }
        if managerId == "rustup",
           let toolchain = labelArgs?["toolchain"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !toolchain.isEmpty {
            return "rustup:\(toolchain)"
        }
        guard let managerId,
              let package = labelArgs?["package"]?.trimmingCharacters(in: .whitespacesAndNewlines),
              !package.isEmpty else {
            return nil
        }
        return "\(managerId):\(package)"
    }

    static func projectedTaskIdsForCancellation(
        scopedStepIds: Set<String>,
        projections: [String: ProjectedTaskState]
    ) -> Set<Int64> {
        var taskIds = Set<Int64>()
        for stepId in scopedStepIds {
            guard let projection = projections[stepId] else { continue }
            guard isInFlightStatus(status: projection.status, hasProjectedTask: true) else { continue }
            guard let taskId = Int64(exactly: projection.taskId) else { continue }
            taskIds.insert(taskId)
        }
        return taskIds
    }

    private static func includeForUpgradePreview(
        candidate: Candidate,
        managerEnabled: [String: Bool],
        includePinned: Bool,
        allowOsUpdates: Bool,
        safeModeEnabled: Bool
    ) -> Bool {
        guard includePinned || !candidate.pinned else { return false }
        guard managerEnabled[candidate.managerId] ?? true else { return false }
        if candidate.managerId == "softwareupdate" && !allowOsUpdates {
            return false
        }
        if candidate.managerId == "softwareupdate" && safeModeEnabled {
            return false
        }
        return true
    }
}

struct PackageConsolidationPolicy {
    static func statusRank(_ rawStatus: String) -> Int {
        switch rawStatus.lowercased() {
        case "upgradable":
            return 0
        case "installed":
            return 1
        case "available":
            return 2
        default:
            return 3
        }
    }

    static func sortedManagerIds(
        _ managerIds: [String],
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)? = nil
    ) -> [String] {
        Array(Set(managerIds)).sorted { lhs, rhs in
            if let priorityRank {
                let lhsPriority = priorityRank(lhs)
                let rhsPriority = priorityRank(rhs)
                if lhsPriority != rhsPriority {
                    return lhsPriority < rhsPriority
                }
            }
            return localizedManagerName(lhs).localizedCaseInsensitiveCompare(localizedManagerName(rhs)) == .orderedAscending
        }
    }

    static func preferredManagerId(
        managerIds: [String],
        preferredManagerId: String?
    ) -> String? {
        guard !managerIds.isEmpty else { return nil }
        if let preferredManagerId, managerIds.contains(preferredManagerId) {
            return preferredManagerId
        }
        return managerIds[0]
    }

    static func shouldPrefer(
        lhsStatus: String,
        rhsStatus: String,
        lhsPinned: Bool,
        rhsPinned: Bool,
        lhsRestartRequired: Bool,
        rhsRestartRequired: Bool,
        lhsVersion: String? = nil,
        rhsVersion: String? = nil,
        lhsManagerId: String,
        rhsManagerId: String,
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)? = nil
    ) -> Bool {
        let lhsRank = statusRank(lhsStatus)
        let rhsRank = statusRank(rhsStatus)
        if lhsRank != rhsRank {
            return lhsRank < rhsRank
        }
        if lhsPinned != rhsPinned {
            return lhsPinned
        }
        if lhsRestartRequired != rhsRestartRequired {
            return lhsRestartRequired
        }

        let lhsVersionToken = normalizedVersionToken(lhsVersion)
        let rhsVersionToken = normalizedVersionToken(rhsVersion)
        if lhsVersionToken != rhsVersionToken {
            if lhsVersionToken == nil {
                return false
            }
            if rhsVersionToken == nil {
                return true
            }
            if let lhsVersionToken, let rhsVersionToken {
                let order = lhsVersionToken.compare(
                    rhsVersionToken,
                    options: [.numeric, .caseInsensitive]
                )
                if order != .orderedSame {
                    return order == .orderedDescending
                }
            }
        }

        if let priorityRank {
            let lhsPriority = priorityRank(lhsManagerId)
            let rhsPriority = priorityRank(rhsManagerId)
            if lhsPriority != rhsPriority {
                return lhsPriority < rhsPriority
            }
        }
        return localizedManagerName(lhsManagerId)
            .localizedCaseInsensitiveCompare(localizedManagerName(rhsManagerId)) == .orderedAscending
    }

    private static func normalizedVersionToken(_ value: String?) -> String? {
        guard let value else { return nil }
        let token = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return token.isEmpty ? nil : token
    }
}

enum PackageActionTracking {
    static func normalizedPackageName(_ value: String) -> String {
        PackageActionIdentity.normalizedBaseName(value)
    }

    static func normalizedPackageIdentityKey(name: String, version: String?) -> String {
        PackageActionIdentity.normalizedIdentityKey(name: name, version: version)
    }

    static func packageNameFromPackageId(_ packageId: String) -> String? {
        let normalizedId = packageId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedId.isEmpty,
              let managerSeparator = normalizedId.firstIndex(of: ":"),
              managerSeparator < normalizedId.index(before: normalizedId.endIndex) else {
            return nil
        }

        var versionSlice: Substring?
        var packageSlice = normalizedId[normalizedId.index(after: managerSeparator)...]
        if let versionSeparator = packageSlice.range(of: "::", options: .backwards) {
            versionSlice = packageSlice[versionSeparator.upperBound...]
            packageSlice = packageSlice[..<versionSeparator.lowerBound]
        }

        let packageName = String(packageSlice).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !packageName.isEmpty else { return nil }
        let version = versionSlice.map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
        let normalizedPackageIdentity = normalizedPackageIdentityKey(
            name: packageName,
            version: version
        )
        return normalizedPackageIdentity.isEmpty ? nil : normalizedPackageIdentity
    }

    static func inFlightInstallNames(
        installActionPackageIds: Set<String>,
        packageNameById: [String: String],
        trackedNamesByPackageId: [String: String]
    ) -> Set<String> {
        var names = Set<String>()

        for packageId in installActionPackageIds {
            if let tracked = trackedNamesByPackageId[packageId], !tracked.isEmpty {
                names.insert(tracked)
                continue
            }
            if let mapped = packageNameById[packageId], !mapped.isEmpty {
                names.insert(mapped)
                continue
            }
            if let parsed = packageNameFromPackageId(packageId) {
                names.insert(parsed)
            }
        }

        return names
    }
}

private enum PackageActionIdentity {
    private static let unknownVersionTokens: Set<String> = ["unknown"]

    static func normalizedBaseName(_ value: String) -> String {
        value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    static func normalizedIdentityKey(name: String, version: String?) -> String {
        let normalizedName = normalizedBaseName(name)
        guard !normalizedName.isEmpty else { return "" }
        guard let qualifier = normalizedVariantQualifier(fromVersion: version) else {
            return normalizedName
        }
        return "\(normalizedName)@\(qualifier)"
    }

    private static func normalizedVariantQualifier(fromVersion version: String?) -> String? {
        guard let normalizedVersion = normalizedVersionSelectorInput(version) else { return nil }
        return qualifierFromSelector(normalizedVersion)
    }

    private static func normalizedVersionSelectorInput(_ value: String?) -> String? {
        guard let value else { return nil }
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else { return nil }
        if unknownVersionTokens.contains(normalized.lowercased()) {
            return nil
        }
        return normalized
    }

    private static func qualifierFromSelector(_ selector: String) -> String? {
        let atoms = selector
            .split(separator: "-")
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        guard !atoms.isEmpty else { return nil }

        let firstReleaseAtom = atoms.firstIndex(where: { atom in
            isReleaseTokenAtom(atom)
        })
        let qualifierAtoms: ArraySlice<String> = {
            guard let firstReleaseAtom else {
                return atoms[atoms.startIndex..<atoms.endIndex]
            }
            guard firstReleaseAtom > 0 else { return [] }
            return atoms[atoms.startIndex..<firstReleaseAtom]
        }()
        guard !qualifierAtoms.isEmpty else { return nil }
        return qualifierAtoms.joined(separator: "-").lowercased()
    }

    private static func isReleaseTokenAtom(_ atom: String) -> Bool {
        guard let first = atom.first else { return false }
        if first.isNumber {
            return true
        }
        if first == "v" || first == "V" {
            let next = atom.dropFirst().first
            return next?.isNumber == true
        }
        return false
    }
}
