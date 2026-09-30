import XCTest

final class FirstRunEvidencePresentationTests: XCTestCase {
    func testRepairViewOwnsItsLocaleObservationWithoutResettingIdentity() throws {
        let macOSRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent()
        let source = try String(
            contentsOf: macOSRoot.appendingPathComponent("Helm/Views/Onboarding/ProductionFirstRunView.swift"),
            encoding: .utf8
        )
        let start = try XCTUnwrap(source.range(of: "private struct ProductionFirstRunRepairView: View {"))
        let repairView = String(source[start.upperBound...])
        let body = try XCTUnwrap(repairView.range(of: "var body: some View"))
        let properties = String(repairView[..<body.lowerBound])

        // The parent already observes locale changes, but SwiftUI can retain an
        // unchanged child. Protect the subscription at the view that reads copy.
        XCTAssertTrue(properties.contains("@ObservedObject private var localization = LocalizationManager.shared"))
        XCTAssertFalse(repairView.contains(".id(localization"), "Language changes must not reset repair presentation state")
    }

    private func manager(
        _ id: String,
        paths: [String] = [],
        status: String = "complete",
        enabled: Bool? = nil,
        savedPath: String? = nil,
        cachedInstalled: Bool? = nil
    ) -> FirstRunLocalEvidence.Manager {
        .init(managerId: id, configuredEnabled: enabled, selectedExecutablePath: savedPath,
              candidateScanStatus: status, inspectedPathCount: max(4, paths.count), candidatePaths: paths,
              cachedDetection: cachedInstalled.map { .init(installed: $0, executablePath: nil, version: nil) })
    }

    private func presentation(_ managers: [FirstRunLocalEvidence.Manager]) -> FirstRunEvidencePresentation {
        .init(.init(schemaVersion: 1, experienceId: "wayfinder-v0.20", managers: managers))
    }

    func testFileCountCountsSourcesNotCandidatePathsOrVerifiedManagers() {
        let result = presentation([
            manager("mise", paths: ["/a/shim", "/a/tool", "/b/tool"]),
            manager("cargo", cachedInstalled: true),
            manager("npm", paths: ["/a/npm"], enabled: false)
        ])
        XCTAssertEqual(result.sourcesWithFiles, 2)
        XCTAssertEqual(result.observed.map(\.id), ["mise", "cargo", "npm"])
        XCTAssertEqual(result.savedDisabled, 1)
    }

    func testUnknownPreferencesAreNotConvertedToEnabledOrDisabled() {
        let result = presentation([
            manager("mise", enabled: true), manager("cargo", enabled: false), manager("npm")
        ])
        XCTAssertEqual(result.savedEnabled, 1)
        XCTAssertEqual(result.savedDisabled, 1)
        XCTAssertEqual(result.sourcesWithFiles, 0)
        XCTAssertTrue(result.observed.isEmpty)
        XCTAssertEqual(result.other.count, 3)
    }

    func testPartialUnsupportedAndHistoricalEvidenceRemainDistinct() {
        let result = presentation([
            manager("mise", status: "partial"), manager("sparkle", status: "not_supported"),
            manager("cargo", cachedInstalled: true), manager("npm", savedPath: "/old/npm"),
            manager("yarn", cachedInstalled: false)
        ])
        XCTAssertEqual(result.incompleteScans, 1)
        XCTAssertEqual(result.observed.map(\.id), ["mise", "cargo", "npm"])
        XCTAssertEqual(result.other.map(\.id), ["sparkle", "yarn"])
        XCTAssertEqual(result.sourcesWithFiles, 0)
    }

    func testRowDescriptionsDoNotTurnAbsenceIntoNotInstalledOrFilesIntoReadiness() {
        XCTAssertEqual(FirstRunEvidencePresentation.observationKey(manager("mise")), "app.first_run.evidence.no_files")
        XCTAssertEqual(FirstRunEvidencePresentation.observationKey(manager("sparkle", status: "not_supported")),
                       "app.first_run.evidence.not_scanned")
        XCTAssertEqual(FirstRunEvidencePresentation.observationKey(manager("mise", status: "partial")),
                       "app.first_run.entry.partial")
        XCTAssertEqual(FirstRunEvidencePresentation.observationKey(manager("mise", paths: ["/shim"])),
                       "app.first_run.entry.candidates")
    }

    func testDisplayPartitionRetainsEverySourceOnceInServiceOrder() {
        let managers = (0..<29).map { index in
            manager("source-\(index)", paths: index.isMultiple(of: 3) ? ["/file/\(index)"] : [])
        }
        let result = presentation(managers)
        XCTAssertEqual(Set((result.observed + result.other).map(\.id)), Set(managers.map(\.id)))
        XCTAssertEqual(result.observed.count + result.other.count, managers.count)
        XCTAssertEqual(result.observed.map(\.id), managers.filter(FirstRunEvidencePresentation.hasVisibleEvidence).map(\.id))
    }

    func testEvidenceCopyIsTranslatedMirroredAndPreservesCountPlaceholders() throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        func strings(_ directory: String, _ locale: String) throws -> [String: String] {
            let data = try Data(contentsOf: root.appendingPathComponent("\(directory)/\(locale)/app.json"))
            return try JSONDecoder().decode([String: String].self, from: data)
        }
        let english = try strings("locales", "en")
        for locale in ["en", "de", "es", "fr", "hu", "ja", "pt-BR"] {
            let catalog = try strings("locales", locale)
            let bundled = try strings("apps/macos-ui/Helm/Resources/locales", locale)
            for key in FirstRunEvidencePresentation.localizationKeys {
                let value = try XCTUnwrap(catalog[key], "\(locale): \(key)")
                XCTAssertFalse(value.isEmpty)
                XCTAssertEqual(value, bundled[key])
                if locale != "en" { XCTAssertNotEqual(value, english[key], "\(locale): \(key)") }
            }
            for suffix in ["other", "partial"] {
                XCTAssertTrue(try XCTUnwrap(catalog["app.first_run.evidence.\(suffix)"]).contains("{count}"))
            }
        }
    }
}
