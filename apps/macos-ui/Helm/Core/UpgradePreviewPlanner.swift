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
