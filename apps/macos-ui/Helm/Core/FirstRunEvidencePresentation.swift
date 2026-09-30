import Foundation

/// Display-only grouping of metadata evidence, never manager eligibility or health.
struct FirstRunEvidencePresentation {
    typealias Manager = FirstRunLocalEvidence.Manager

    let observed: [Manager]
    let other: [Manager]
    let sourcesWithFiles: Int
    let savedEnabled: Int
    let savedDisabled: Int
    let incompleteScans: Int

    init(_ evidence: FirstRunLocalEvidence) {
        observed = evidence.managers.filter(Self.hasVisibleEvidence)
        other = evidence.managers.filter { !Self.hasVisibleEvidence($0) }
        sourcesWithFiles = evidence.managers.filter { !$0.candidatePaths.isEmpty }.count
        savedEnabled = evidence.managers.filter { $0.configuredEnabled == true }.count
        savedDisabled = evidence.managers.filter { $0.configuredEnabled == false }.count
        incompleteScans = evidence.managers.filter { $0.candidateScanStatus == "partial" }.count
    }

    static func hasVisibleEvidence(_ manager: Manager) -> Bool {
        !manager.candidatePaths.isEmpty || manager.cachedDetection?.installed == true
            || manager.selectedExecutablePath != nil || manager.candidateScanStatus == "partial"
    }

    static func observationKey(_ manager: Manager) -> String {
        if !manager.candidatePaths.isEmpty { return "app.first_run.entry.candidates" }
        if manager.candidateScanStatus == "not_supported" { return "app.first_run.evidence.not_scanned" }
        if manager.candidateScanStatus == "partial" { return "app.first_run.entry.partial" }
        return "app.first_run.evidence.no_files"
    }

    static let localizationKeys = [
        "app.first_run.evidence.title", "app.first_run.evidence.summary",
        "app.first_run.evidence.file_sources", "app.first_run.evidence.preferences",
        "app.first_run.evidence.enabled", "app.first_run.evidence.disabled",
        "app.first_run.evidence.unchanged", "app.first_run.evidence.other",
        "app.first_run.evidence.no_files", "app.first_run.evidence.not_scanned",
        "app.first_run.evidence.paths", "app.first_run.evidence.saved_path",
        "app.first_run.evidence.empty", "app.first_run.evidence.partial"
    ]
}
