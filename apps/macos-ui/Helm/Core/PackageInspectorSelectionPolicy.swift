import Foundation

enum PackageInspectorSelectionPolicy {
    static func hasExactPackageSelection(
        selectedPackageId: String?,
        presentedPackageId: String,
        presentedManagerId: String,
        candidateManagerIds: Set<String>
    ) -> Bool {
        selectedPackageId == presentedPackageId
            && candidateManagerIds.contains(presentedManagerId)
    }

    static func managerId(
        explicitManagerId: String?,
        selectedPackageManagerId: String?,
        persistedManagerId: String?,
        candidateManagerIds: Set<String>,
        fallbackManagerId: String
    ) -> String {
        if let explicitManagerId, candidateManagerIds.contains(explicitManagerId) {
            return explicitManagerId
        }
        if let selectedPackageManagerId, candidateManagerIds.contains(selectedPackageManagerId) {
            return selectedPackageManagerId
        }
        if let persistedManagerId, candidateManagerIds.contains(persistedManagerId) {
            return persistedManagerId
        }
        return fallbackManagerId
    }
}

struct PackageMemberIdentity: Equatable {
    let packageID: String
    let managerID: String
}

struct PackageMemberSelection: Equatable {
    let managerID: String
    let packageID: String
}

enum PackageMemberSelectionPolicy {
    static func actionTarget(
        members: [PackageMemberIdentity],
        orderedManagerIDs: [String],
        preferredManagerID: String?,
        selectedPackageID: String?
    ) -> PackageMemberIdentity? {
        if let selectedPackageID,
           let selectedMember = members.first(where: { $0.packageID == selectedPackageID }) {
            return selectedMember
        }
        guard let managerID = PackageConsolidationPolicy.preferredManagerId(
            managerIds: orderedManagerIDs,
            preferredManagerId: preferredManagerID
        ) else {
            return members.first
        }
        return members.first(where: { $0.managerID == managerID }) ?? members.first
    }

    static func initialInstallSelection(
        candidates: [PackageMemberIdentity],
        managerConstraint: String?,
        preferredManagerID: String?,
        selectedPackageID: String?
    ) -> PackageMemberSelection? {
        let normalizedConstraint = managerConstraint?.trimmingCharacters(in: .whitespacesAndNewlines)
        let eligibleCandidates = candidates.filter { candidate in
            guard let normalizedConstraint, !normalizedConstraint.isEmpty else { return true }
            return candidate.managerID == normalizedConstraint
        }
        guard !eligibleCandidates.isEmpty else { return nil }

        if let selectedPackageID,
           let selectedCandidate = eligibleCandidates.first(where: { $0.packageID == selectedPackageID }) {
            return PackageMemberSelection(
                managerID: selectedCandidate.managerID,
                packageID: selectedCandidate.packageID
            )
        }

        let managerID = PackageConsolidationPolicy.preferredManagerId(
            managerIds: eligibleCandidates.map(\.managerID),
            preferredManagerId: normalizedConstraint ?? preferredManagerID
        ) ?? eligibleCandidates[0].managerID
        let member = eligibleCandidates.first(where: { $0.managerID == managerID })
            ?? eligibleCandidates[0]
        return PackageMemberSelection(managerID: member.managerID, packageID: member.packageID)
    }
}
