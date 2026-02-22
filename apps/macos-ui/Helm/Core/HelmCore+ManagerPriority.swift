import Foundation

extension HelmCore {
    func managerPriorityRank(for managerId: String) -> Int {
        guard let manager = ManagerInfo.find(byId: managerId) else {
            return Int.max / 2
        }
        let localRank = managerPriorityOverrides[managerId] ?? ManagerInfo.defaultPriorityRank(for: managerId)
        // Preserve explicit authority ordering when comparing managers globally.
        let authorityBase: Int
        switch manager.authority {
        case .authoritative:
            authorityBase = 0
        case .standard:
            authorityBase = 1
        case .guarded:
            authorityBase = 2
        }
        return authorityBase * 10_000 + localRank
    }

    func sortedManagersByPriority(_ managers: [ManagerInfo]) -> [ManagerInfo] {
        managers.sorted { lhs, rhs in
            let lhsDetected = managerStatuses[lhs.id]?.detected ?? false
            let rhsDetected = managerStatuses[rhs.id]?.detected ?? false
            if lhsDetected != rhsDetected {
                return lhsDetected && !rhsDetected
            }

            let lhsRank = managerPriorityRank(for: lhs.id)
            let rhsRank = managerPriorityRank(for: rhs.id)
            if lhsRank != rhsRank {
                return lhsRank < rhsRank
            }

            return localizedManagerDisplayName(lhs.id)
                .localizedCaseInsensitiveCompare(localizedManagerDisplayName(rhs.id)) == .orderedAscending
        }
    }

    func moveManagerPriority(
        authority: ManagerAuthority,
        draggedManagerId: String,
        targetManagerId: String
    ) {
        guard draggedManagerId != targetManagerId else { return }
        guard let dragged = ManagerInfo.find(byId: draggedManagerId),
              let target = ManagerInfo.find(byId: targetManagerId),
              dragged.authority == authority,
              target.authority == authority else { return }

        let draggedDetected = managerStatuses[draggedManagerId]?.detected ?? false
        let targetDetected = managerStatuses[targetManagerId]?.detected ?? false
        guard draggedDetected == targetDetected else { return }

        var installedOrder = priorityOrderedIds(for: authority, detected: true)
        var missingOrder = priorityOrderedIds(for: authority, detected: false)

        if draggedDetected {
            moveManagerId(
                in: &installedOrder,
                draggedManagerId: draggedManagerId,
                targetManagerId: targetManagerId
            )
        } else {
            moveManagerId(
                in: &missingOrder,
                draggedManagerId: draggedManagerId,
                targetManagerId: targetManagerId
            )
        }

        applyPriorityOrder(
            authority: authority,
            installedOrder: installedOrder,
            missingOrder: missingOrder
        )
    }

    func restoreDefaultManagerPriorities() {
        managerPriorityOverrides = [:]
        persistManagerPriorityOverrides()
    }

    private func priorityOrderedIds(for authority: ManagerAuthority, detected: Bool) -> [String] {
        let managers = ManagerInfo.all
            .filter { $0.authority == authority }
            .filter { managerStatuses[$0.id]?.detected ?? false == detected }
        return sortedManagersByPriority(managers).map(\.id)
    }

    private func moveManagerId(
        in orderedIds: inout [String],
        draggedManagerId: String,
        targetManagerId: String
    ) {
        guard let sourceIndex = orderedIds.firstIndex(of: draggedManagerId),
              let targetIndex = orderedIds.firstIndex(of: targetManagerId) else { return }
        let moved = orderedIds.remove(at: sourceIndex)
        orderedIds.insert(moved, at: targetIndex)
    }

    private func applyPriorityOrder(
        authority: ManagerAuthority,
        installedOrder: [String],
        missingOrder: [String]
    ) {
        var overrides = managerPriorityOverrides
        let managerIds = ManagerInfo.all
            .filter { $0.authority == authority }
            .map(\.id)

        let finalOrder = installedOrder + missingOrder
        for (index, managerId) in finalOrder.enumerated() {
            overrides[managerId] = index
        }

        // Remove stale values for managers that no longer exist in this authority.
        for managerId in managerIds where !finalOrder.contains(managerId) {
            overrides.removeValue(forKey: managerId)
        }

        managerPriorityOverrides = overrides
        persistManagerPriorityOverrides()
    }

    private func persistManagerPriorityOverrides() {
        guard let data = try? JSONEncoder().encode(managerPriorityOverrides) else {
            return
        }
        UserDefaults.standard.set(data, forKey: Self.managerPriorityOverridesKey)
    }
}
