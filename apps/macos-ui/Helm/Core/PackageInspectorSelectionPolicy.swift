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
