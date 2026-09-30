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
            let lhsDetected = isManagerDetected(lhs.id)
            let rhsDetected = isManagerDetected(rhs.id)
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

    func installedManagerPriorityOrder(for authority: ManagerAuthority) -> [String] {
        priorityOrderedIds(for: authority, detected: true)
    }

    @discardableResult
    func commitManagerPriorityMove(_ session: ManagerPriorityReorderSession, authority: ManagerAuthority) -> Bool {
        let current = installedManagerPriorityOrder(for: authority)
        guard let proposed = session.validatedOrder(currentInstalledOrder: current, authorityKey: authority.key) else { return false }
        guard proposed != current else { return true }
        applyPriorityOrder(
            authority: authority,
            installedOrder: proposed,
            missingOrder: priorityOrderedIds(for: authority, detected: false)
        )
        return true
    }

    func restoreDefaultManagerPriorities() {
        managerPriorityOverrides = [:]
        persistManagerPriorityOverrides()
    }

    private func priorityOrderedIds(for authority: ManagerAuthority, detected: Bool) -> [String] {
        let managers = ManagerInfo.all
            .filter { $0.authority == authority }
            .filter { isManagerDetected($0.id) == detected }
        return sortedManagersByPriority(managers).map(\.id)
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
