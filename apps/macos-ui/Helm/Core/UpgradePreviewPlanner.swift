import Foundation

struct UpgradePreviewPlanner {
    struct Entry: Equatable {
        let manager: String
        let count: Int
    }

    struct Candidate {
        let managerId: String
        let pinned: Bool
    }

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
