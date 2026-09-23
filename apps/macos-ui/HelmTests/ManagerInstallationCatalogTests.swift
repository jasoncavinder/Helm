import XCTest

final class ManagerInstallationCatalogTests: XCTestCase {
    typealias Catalog = ManagerInstallationCatalog

    private func source(
        _ id: String = "mise", detected: Bool = false, enabled: Bool = true,
        implemented: Bool = true, eligible: Bool = true, busy: Bool = false,
        methods: [Catalog.Method] = [.init(id: "scriptInstaller", policyAllowed: true, dependencyID: nil)]
    ) -> Catalog.Source {
        Catalog.Source(id: id, detected: detected, enabled: enabled,
                       implemented: implemented, eligible: eligible, busy: busy, methods: methods)
    }

    private func catalog(_ sources: [Catalog.Source]) -> Catalog {
        Catalog(sources: sources, connected: true, online: true, research: false)
    }

    func testOnlyUndetectedImplementedPlannerSupportedManagersAreCandidates() {
        let value = catalog([
            source("mise"), source("installed", detected: true),
            source("unimplemented", implemented: false), source("unsupported", methods: [])
        ])
        XCTAssertEqual(value.candidates.map(\.id), ["mise"])
        XCTAssertTrue(value.canReview("mise"))
    }

    func testDisabledUndetectedManagerCanBeInstalledWithoutEnablingItFirst() {
        XCTAssertTrue(catalog([source(enabled: false)]).canReview("mise"))
    }

    func testMissingCoreMethodsCannotFallBackToUIRegistry() {
        let value = catalog([source(methods: [])])
        XCTAssertTrue(value.candidates.isEmpty)
        XCTAssertFalse(value.canReview("mise"))
    }

    func testPolicyBlockedMethodsStayVisibleButCannotBeReviewed() {
        let value = catalog([source(methods: [.init(id: "scriptInstaller", policyAllowed: false, dependencyID: nil)])])
        XCTAssertEqual(value.candidates.first?.methods.first?.block, .policy)
        XCTAssertFalse(value.canReview("mise"))
    }

    func testIneligibleManagerCannotUseAnOtherwiseAllowedMethod() {
        let value = catalog([source(eligible: false)])
        XCTAssertEqual(value.candidates.first?.methods.first?.block, .policy)
        XCTAssertFalse(value.canReview("mise"))
    }

    func testDependencyMustBeDetectedEnabledImplementedEligibleAndIdle() {
        let candidate = source(methods: [.init(id: "homebrew", policyAllowed: true, dependencyID: "homebrew_formula")])
        let dependencies: [Catalog.Source?] = [
            nil, source("homebrew_formula"),
            source("homebrew_formula", detected: true, enabled: false),
            source("homebrew_formula", detected: true, implemented: false),
            source("homebrew_formula", detected: true, eligible: false),
            source("homebrew_formula", detected: true, busy: true)
        ]
        for dependency in dependencies {
            let value = catalog([candidate] + [dependency].compactMap { $0 })
            XCTAssertEqual(value.candidates.first(where: { $0.id == "mise" })?.methods.first?.block,
                           .dependency("homebrew_formula"))
            XCTAssertFalse(value.canReview("mise"))
        }
        XCTAssertTrue(catalog([candidate, source("homebrew_formula", detected: true)]).canReview("mise"))
    }

    func testAlternativeDirectMethodDoesNotRequireHomebrew() {
        let value = catalog([source(methods: [
            .init(id: "homebrew", policyAllowed: true, dependencyID: "homebrew_formula"),
            .init(id: "scriptInstaller", policyAllowed: true, dependencyID: nil)
        ])])
        XCTAssertTrue(value.canReview("mise"))
        XCTAssertEqual(value.candidates.first?.methods.map(\.block), [.dependency("homebrew_formula"), nil])
    }

    func testBusyManagerCannotStartAnotherInstallation() {
        let value = catalog([source(busy: true)])
        XCTAssertEqual(value.candidates.first?.methods.first?.block, .busy)
        XCTAssertFalse(value.canReview("mise"))
    }

    func testDisconnectedOfflineAndResearchStatesDisableReview() {
        let states: [(Bool, Bool, Bool, Catalog.Availability)] = [
            (false, true, false, .disconnected), (true, false, false, .offline),
            (true, true, true, .research), (false, false, true, .research)
        ]
        for (connected, online, research, expected) in states {
            let value = Catalog(sources: [source()], connected: connected, online: online, research: research)
            XCTAssertEqual(value.availability, expected)
            XCTAssertEqual(value.candidates.count, 1)
            XCTAssertFalse(value.canReview("mise"))
        }
    }

    func testInitialLoadingIsNotReportedAsNoSupportedInstallations() {
        let loading = catalog([])
        let empty = catalog([source(detected: true)])
        XCTAssertEqual(loading.availability, .loading)
        XCTAssertEqual(empty.availability, .ready)
        XCTAssertTrue(empty.candidates.isEmpty)
    }

    func testStaleSelectionIsRevalidatedAgainstCurrentSnapshot() {
        XCTAssertTrue(catalog([source()]).canReview("mise"))
        for changed in [source(detected: true), source(eligible: false), source(busy: true), source(methods: [])] {
            XCTAssertFalse(catalog([changed]).canReview("mise"))
        }
        XCTAssertFalse(catalog([]).canReview("mise"))
        XCTAssertFalse(catalog([source()]).canReview(nil))
        XCTAssertFalse(catalog([source()]).canReview("unknown"))
    }

    func testStableOrderingDoesNotDependOnDictionaryIteration() {
        XCTAssertEqual(catalog([source("yarn"), source("asdf"), source("mise")]).candidates.map(\.id),
                       ["asdf", "mise", "yarn"])
    }

    func testSelectedMethodCannotBeReplacedByAnotherAvailableMethod() {
        let value = catalog([source(methods: [
            .init(id: "homebrew", policyAllowed: true, dependencyID: "homebrew_formula"),
            .init(id: "scriptInstaller", policyAllowed: true, dependencyID: nil)
        ])])
        XCTAssertTrue(value.canReview("mise"))
        XCTAssertFalse(value.canReview("mise", methodID: "homebrew"))
        XCTAssertFalse(value.canReview("mise", methodID: "removedMethod"))
        XCTAssertTrue(value.canReview("mise", methodID: "scriptInstaller"))
    }
}
