import Cocoa
import SwiftUI
import Combine
import UserNotifications
import os.log

private let appDelegateLogger = Logger(
    subsystem: "com.jasoncavinder.Helm",
    category: "app_delegate"
)

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate, UNUserNotificationCenterDelegate {
    private static let controlCenterFrameAutosaveName = "HelmDashboardWindow"
    private var statusItem: NSStatusItem?
    private var panel: FloatingPanel!
    private var eventMonitor: EventMonitor?
    private var controlCenterWindowController: NSWindowController?
    private var settingsWindowController: NSWindowController?
    private var cancellables: Set<AnyCancellable> = []

    private let core = HelmCore.shared
    let controlCenterContext = ControlCenterContext()
    private let notificationCenter = UNUserNotificationCenter.current()
    private var observedUpdateFingerprint: String?
    private var announcedTimeoutPromptIds: Set<String> = []
    private var announcedUpgradePlanCompletionWorkflowId: String?
    private static let timeoutPromptCategoryId = "helm.task.timeout.prompt"
    private static let timeoutPromptActionWaitId = "helm.task.timeout.prompt.wait"
    private static let timeoutPromptActionStopId = "helm.task.timeout.prompt.stop"
    private static let timeoutPromptTaskIdUserInfoKey = "task_id"
    private static let timeoutPromptIdUserInfoKey = "prompt_id"
    private static let upgradePlanCompletionCategoryId = "helm.upgrade-plan.completed"
    private static let updatesAvailableReviewCategoryId = "helm.updates.available.review"
    private static let updatesAvailableUpgradeCategoryId = "helm.updates.available.upgrade"
    private static let reviewPlanActionId = "helm.updates.review-plan"
    private static let upgradeAllActionId = "helm.updates.upgrade-all"
    private static let updatesAvailableNotificationId = "helm.updates.available"
    private var isControlCenterVisible: Bool {
        controlCenterWindowController?.window?.isVisible == true
    }

    private var presentsEnvironmentBrief: Bool {
        controlCenterContext.shouldPresentFirstRun(
            mode: EnvironmentBriefFirstRunConfiguration.mode(),
            hasCompletedOnboarding: core.hasCompletedOnboarding
        )
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        #if DEBUG
        if runLibraryPerformanceBenchmarkIfRequested() {
            return
        }
        #endif
        NSApp.setActivationPolicy(.accessory)
        controlCenterContext.settingsOpenRouter.configure { [weak self] in
            self?.openSettingsWindow()
        }

        let contentView = WayfinderPopoverView(onOpenControlCenter: { [weak self] in
            self?.openControlCenter()
            self?.closePanel()
        }, onOpenSettings: { [weak self] in
            guard let self else { return }
            controlCenterContext.settingsOpenRouter.requestOpen()
            closePanel()
        }, onClosePopover: { [weak self] in
            self?.closePanel()
        })
        .environmentObject(controlCenterContext)
        .background(VisualEffect().ignoresSafeArea())
        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))

        panel = FloatingPanel(
            contentRect: NSRect(
                x: 0,
                y: 0,
                width: WayfinderPopoverLayout.width,
                height: WayfinderPopoverLayout.ordinaryHeight
            ),
            backing: .buffered,
            defer: false
        )
        panel.onCommandF = { [weak self] in
            self?.focusControlCenterSearch()
        }
        panel.onEscape = { [weak self] in
            self?.closePanel()
        }
        panel.contentViewController = NSHostingController(rootView: contentView)

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem?.button {
            button.image = statusItemImage(
                anchorTint: menuBaseTint(for: button),
                button: button,
                badge: nil
            )
            button.action = #selector(togglePanel(_:))
            button.target = self
            button.sendAction(on: [.leftMouseUp, .rightMouseUp])
        }
        bindStatusItem()
        updateStatusItemAppearance()
        configureUserNotifications()

        eventMonitor = EventMonitor(
            mask: [.leftMouseDown, .rightMouseDown],
            localHandler: { [weak self] event in
                guard let self else { return event }
                return self.handlePanelLocalEvent(event)
            },
            globalHandler: { [weak self] event in
                self?.handlePanelGlobalEvent(event)
            }
        )

        DistributedNotificationCenter.default().addObserver(
            self,
            selector: #selector(handleSystemAppearanceChanged),
            name: Notification.Name("AppleInterfaceThemeChangedNotification"),
            object: nil
        )
        core.refreshLaunchAtLogin()
        core.setInteractiveSurfaceVisibility(popoverVisible: false, controlCenterVisible: false)

        let firstRunMode = EnvironmentBriefFirstRunConfiguration.mode()
        let fixtureActive = EnvironmentBriefFixtureProvider.active() != nil
        let researchFixtureActive = ResearchFixtureSafetyPolicy.blocksLiveOperations()

        if core.hasCompletedOnboarding
            && !core.requiresLicenseTermsAcceptance
            && !researchFixtureActive
            && EnvironmentBriefFirstRunConfiguration.allowsAutomaticRefresh(
                mode: firstRunMode,
                fixtureActive: fixtureActive
            ) {
            core.triggerRefresh()
        }

        if firstRunMode == .preview {
            DispatchQueue.main.async { [weak self] in
                self?.openControlCenter()
            }
        }

        #if DEBUG
        runPopoverOpeningBenchmarkIfRequested()
        #endif
    }

    func openDashboardFromApplicationMenu() {
        controlCenterContext.navigate(
            to: WayfinderDeepLink(
                destination: .dashboard,
                entityID: nil,
                focus: .primaryContent
            )
        )
        openControlCenter()
    }

    func selectSectionFromApplicationMenu(_ section: ControlCenterSection) {
        controlCenterContext.select(section)
        openControlCenter()
    }

    func toggleSidebarFromApplicationMenu() {
        openControlCenter()
        controlCenterContext.toggleSidebar()
    }

    func toggleInspectorFromApplicationMenu() {
        openControlCenter()
        controlCenterContext.toggleInspector()
    }

    func focusSearchFromApplicationMenu() {
        focusControlCenterSearch()
    }

    func refreshFromApplicationMenu() {
        core.triggerRefresh()
    }

    private func handlePanelLocalEvent(_ event: NSEvent) -> NSEvent? {
        guard panel.isVisible else { return event }

        let eventPoint: NSPoint
        if let sourceWindow = event.window {
            eventPoint = sourceWindow.convertPoint(toScreen: event.locationInWindow)
        } else {
            eventPoint = NSEvent.mouseLocation
        }

        let clickInPanel = panel.frame.contains(eventPoint)
        let clickInStatusItem = statusItemButtonFrame()?.contains(eventPoint) ?? false
        if !clickInPanel && !clickInStatusItem {
            closePanel()
        }
        return event
    }

    private func handlePanelGlobalEvent(_ event: NSEvent?) {
        guard panel.isVisible else { return }
        guard event?.type == .leftMouseDown || event?.type == .rightMouseDown else { return }

        let clickPoint: NSPoint
        if let event, let sourceWindow = event.window {
            clickPoint = sourceWindow.convertPoint(toScreen: event.locationInWindow)
        } else {
            clickPoint = NSEvent.mouseLocation
        }

        let clickInPanel = panel.frame.contains(clickPoint)
        let clickInStatusItem = statusItemButtonFrame()?.contains(clickPoint) ?? false
        if !clickInPanel && !clickInStatusItem {
            closePanel()
        }
    }

    private func openSettingsWindow() {
        closePanel()

        if settingsWindowController == nil {
            let rootView = SettingsWindowView(onDismiss: { [weak self] in
                self?.settingsWindowController?.close()
            })
            .environmentObject(controlCenterContext)
            let hostingController = NSHostingController(rootView: rootView)
            HelmHostingSizingPolicy.apply(to: hostingController)
            let window = SettingsPanel(
                contentRect: NSRect(
                    origin: .zero,
                    size: HelmPrimaryWindowSizingPolicy.settingsDefaultSize
                )
            )
            window.title = L10n.App.Settings.windowTitle.localized
            window.contentViewController = hostingController
            window.delegate = self
            HelmPrimaryWindowSizingPolicy.applySettings(to: window)
            window.center()
            settingsWindowController = NSWindowController(window: window)
        }

        guard let settingsWindow = settingsWindowController?.window else { return }
        settingsWindow.parent?.removeChildWindow(settingsWindow)
        if let dashboardWindow = controlCenterWindowController?.window,
           dashboardWindow.isVisible {
            dashboardWindow.addChildWindow(settingsWindow, ordered: .above)
        }
        settingsWindow.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func togglePanel(_ sender: AnyObject?) {
        let clickKind: StatusItemClickKind = NSApp.currentEvent?.type == .rightMouseUp
            ? .secondary
            : .primary
        let firstRunMode = EnvironmentBriefFirstRunConfiguration.mode()
        let shouldPresentFirstRun = controlCenterContext.shouldPresentFirstRun(
            mode: firstRunMode,
            hasCompletedOnboarding: core.hasCompletedOnboarding
        )

        switch StatusItemActivationPolicy.route(
            clickKind: clickKind,
            isDashboardVisible: isControlCenterVisible,
            shouldPresentFirstRun: shouldPresentFirstRun
        ) {
        case .dashboard:
            openControlCenter()
            closePanel()
        case .popover:
            if panel.isVisible {
                closePanel()
            } else {
                showPanel(allowWhileControlCenterVisible: clickKind == .secondary)
            }
        }
    }

    private func showPanel(
        allowWhileControlCenterVisible: Bool = false,
        didAcknowledge: (() -> Void)? = nil
    ) {
        guard statusItem?.button != nil else { return }
        guard allowWhileControlCenterVisible || !isControlCenterVisible else {
            openControlCenter()
            return
        }

        let buttonRect = statusItemButtonFrame() ?? .zero

        let panelWidth = WayfinderPopoverLayout.width
        panel.setContentSize(NSSize(width: panelWidth, height: preferredPopoverHeight(forWidth: panelWidth)))

        let panelSize = panel.frame.size
        let x = buttonRect.origin.x + (buttonRect.width / 2) - (panelSize.width / 2)
        let y = buttonRect.origin.y - panelSize.height - 6

        panel.setFrameOrigin(NSPoint(x: x, y: y))
        panel.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        didAcknowledge?()
        eventMonitor?.start()
        core.setInteractiveSurfaceVisibility(popoverVisible: true, controlCenterVisible: isControlCenterVisible)
    }

    #if DEBUG
    private struct LibraryTableBenchmarkResult {
        let frameSamples: [Double]
        let framesWithinSixteenMilliseconds: Int
        let maximumRetainedRowViewCount: Int
    }

    private struct ColdLibraryBenchmarkResult {
        let indexedDerivationSamples: [Double]
        let snapshotApplySamples: [Double]
        let endToEndSamples: [Double]
        let resultCount: Int
        let projectionCacheMisses: Int
        let projectionCache: ProductionLibraryTableProjectionCache
    }

    private struct LibraryBenchmarkProjectionFixture {
        let action: (ConsolidatedPackageItem, PackageItem) -> LibraryTableAction?
        let identity: (Int) -> ProductionLibraryTableProjectionIdentity
    }

    private final class LibraryTableBenchmarkHarness {
        static let retainedRowViewBudget = 20
        static let coldSnapshotApplyP95Budget = 350.0
        static let coldEndToEndP95Budget = 500.0

        private let coordinator: LibraryTableView.Coordinator
        private let tableView: LibraryNativeTableView
        private let scrollView: LibraryTableScrollView
        private let window: NSWindow
        private(set) var maximumRetainedRowViewCount = 0

        init() {
            let parent = Self.makeParent(
                snapshot: .empty(namespace: "benchmark-library")
            )
            let coordinator = parent.makeCoordinator()
            let tableView = LibraryNativeTableView(
                frame: NSRect(x: 0, y: 0, width: 1_000, height: 620)
            )
            tableView.dataSource = coordinator
            tableView.delegate = coordinator
            tableView.intercellSpacing = NSSize(width: 8, height: 2)
            LibraryTableLayoutPolicy.configure(in: tableView)
            coordinator.installColumns(in: tableView)
            coordinator.attach(tableView)

            let scrollView = LibraryTableScrollView(
                frame: NSRect(x: 0, y: 0, width: 1_000, height: 620)
            )
            scrollView.hasVerticalScroller = true
            scrollView.documentView = tableView
            let window = NSWindow(
                contentRect: scrollView.frame,
                styleMask: [.borderless],
                backing: .buffered,
                defer: false
            )
            window.contentView = scrollView

            self.coordinator = coordinator
            self.tableView = tableView
            self.scrollView = scrollView
            self.window = window
            coordinator.update(parent: parent)
        }

        func apply(snapshot: LibraryTableSnapshot) {
            coordinator.update(parent: Self.makeParent(snapshot: snapshot))
            scrollView.contentView.scroll(to: .zero)
            scrollView.reflectScrolledClipView(scrollView.contentView)
            scrollView.layoutSubtreeIfNeeded()
            realizeVisibleCells()
            tableView.layoutSubtreeIfNeeded()
            window.displayIfNeeded()
            recordRetainedRowViews()
        }

        func benchmarkScroll(rowCount: Int) -> LibraryTableBenchmarkResult {
            let lastRowMaxY = rowCount == 0
                ? 0
                : tableView.rect(ofRow: rowCount - 1).maxY
            let maximumOffset = max(
                0,
                lastRowMaxY - scrollView.contentView.bounds.height
            )
            let frameCount = 120
            var frameSamples: [Double] = []
            frameSamples.reserveCapacity(frameCount)
            var framesWithinBudget = 0

            for frame in 0..<frameCount {
                let progress = Double(frame + 1) / Double(frameCount)
                let startedAt = ProcessInfo.processInfo.systemUptime
                scrollView.contentView.scroll(
                    to: NSPoint(x: 0, y: maximumOffset * progress)
                )
                scrollView.reflectScrolledClipView(scrollView.contentView)
                realizeVisibleCells()
                tableView.layoutSubtreeIfNeeded()
                scrollView.layoutSubtreeIfNeeded()
                window.displayIfNeeded()
                recordRetainedRowViews()
                let elapsed = (ProcessInfo.processInfo.systemUptime - startedAt) * 1_000
                frameSamples.append(elapsed)
                if elapsed <= 16.7 {
                    framesWithinBudget += 1
                }
            }

            return LibraryTableBenchmarkResult(
                frameSamples: frameSamples,
                framesWithinSixteenMilliseconds: framesWithinBudget,
                maximumRetainedRowViewCount: maximumRetainedRowViewCount
            )
        }

        func finish() {
            coordinator.detach()
            window.contentView = nil
        }

        private static func makeParent(snapshot: LibraryTableSnapshot) -> LibraryTableView {
            LibraryTableView(
                snapshot: snapshot,
                selectedRowID: nil,
                columnLabels: LibraryTableColumnLabels(
                    package: "Package",
                    manager: "Manager",
                    version: "Version",
                    status: "Status",
                    currentVersion: "Current",
                    latestVersion: "Latest",
                    pinned: "Pinned",
                    restartRequired: "Restart required",
                    running: "Running",
                    viewDetails: "View details"
                ),
                accessibilityLabel: "Library performance fixture",
                focusRequest: nil,
                onSelectRow: { _ in },
                onClearSelection: {},
                onShowDetails: { _ in },
                onPerformAction: { _ in },
                onFulfillFocusRequest: { _ in }
            )
        }

        private func realizeVisibleCells() {
            let visibleRows = tableView.rows(in: tableView.visibleRect)
            guard visibleRows.location != NSNotFound, visibleRows.length > 0 else { return }
            for row in visibleRows.location..<(visibleRows.location + visibleRows.length) {
                for column in tableView.tableColumns.indices {
                    _ = tableView.view(atColumn: column, row: row, makeIfNecessary: true)
                }
            }
        }

        private func recordRetainedRowViews() {
            let retainedCount = tableView.subviews.lazy.filter { $0 is NSTableRowView }.count
            maximumRetainedRowViewCount = max(maximumRetainedRowViewCount, retainedCount)
        }
    }

    private func runLibraryPerformanceBenchmarkIfRequested() -> Bool {
        guard let iterations = LibraryPerformanceBenchmarkConfiguration.iterations() else {
            return false
        }

        Task { @MainActor [core] in
            let sourceCount = 30
            let rowCount = 20_000
            let query = "package"
            let overlaySummary = "Remote same-ID overlay summary marker"
            let packages = self.libraryBenchmarkPackages(rowCount: rowCount, sourceCount: sourceCount)
            core.cachedAllKnownPackagesUnsorted = packages
            core.cachedAllKnownPackagesSorted = packages
            core.cachedKnownPackageById = Dictionary(
                uniqueKeysWithValues: packages.map { ($0.id, $0) }
            )
            var overlayPackage = packages[0]
            overlayPackage.summary = overlaySummary
            core.searchResults = [overlayPackage]
            core.cachedLibraryPackageIndex = nil

            let harness = LibraryTableBenchmarkHarness()
            let projectionFixture = self.libraryBenchmarkProjectionFixture(
                packages: packages,
                sourceCount: sourceCount
            )
            let coldResult = self.runColdLibraryBenchmark(
                iterations: iterations, query: query,
                core: core,
                harness: harness,
                projectionFixture: projectionFixture
            )
            var resultCount = coldResult.resultCount
            var projectionCache = coldResult.projectionCache
            var latestSnapshot = projectionCache.snapshot
            var projectedRowCount = latestSnapshot.rows.count

            _ = core.filteredPackages(
                query: query,
                managerId: nil,
                statusFilter: nil
            )
            var warmIndexedDerivationSamples: [Double] = []
            var warmEndToEndSamples: [Double] = []
            warmIndexedDerivationSamples.reserveCapacity(iterations)
            warmEndToEndSamples.reserveCapacity(iterations)
            var warmProjectionCacheHits = 0
            for _ in 0..<iterations {
                let startedAt = ProcessInfo.processInfo.systemUptime
                let results = core.filteredPackages(
                    query: query,
                    managerId: nil,
                    statusFilter: nil
                )
                let filteredAt = ProcessInfo.processInfo.systemUptime
                warmIndexedDerivationSamples.append((filteredAt - startedAt) * 1_000)
                let sourceFingerprint = LibraryTableRowProjector.sourceFingerprint(
                    for: results
                )
                let projection = projectionCache.resolve(
                    identity: projectionFixture.identity(sourceFingerprint)
                ) { revision in
                    LibraryTableRowProjector.project(
                        revision: revision,
                        packageRows: results,
                        selectedPackageID: nil,
                        managerConstraint: nil,
                        preferredManagerID: { _ in nil },
                        action: projectionFixture.action
                    )
                }
                if projection.cacheHit {
                    warmProjectionCacheHits += 1
                }
                latestSnapshot = projection.snapshot
                harness.apply(snapshot: latestSnapshot)
                resultCount = results.count
                projectedRowCount = latestSnapshot.rows.count
                warmEndToEndSamples.append((ProcessInfo.processInfo.systemUptime - startedAt) * 1_000)
            }

            guard let coldIndexedSummary = LibraryPerformanceSampleSummary(
                samples: coldResult.indexedDerivationSamples
            ), let coldSnapshotSummary = LibraryPerformanceSampleSummary(
                samples: coldResult.snapshotApplySamples
            ), let coldEndToEndSummary = LibraryPerformanceSampleSummary(
                samples: coldResult.endToEndSamples
            ), let warmIndexedSummary = LibraryPerformanceSampleSummary(
                samples: warmIndexedDerivationSamples
            ), let warmEndToEndSummary = LibraryPerformanceSampleSummary(
                samples: warmEndToEndSamples
            ) else {
                harness.finish()
                NSApp.terminate(nil)
                return
            }
            let tableResult = harness.benchmarkScroll(rowCount: projectedRowCount)
            harness.finish()
            guard let frameSummary = LibraryPerformanceSampleSummary(
                samples: tableResult.frameSamples
            ) else {
                NSApp.terminate(nil)
                return
            }
            let framePassRatio = Double(tableResult.framesWithinSixteenMilliseconds)
                / Double(tableResult.frameSamples.count)
            let actionRowCount = latestSnapshot.rows.lazy.filter { $0.action != nil }.count
            let inFlightRowCount = latestSnapshot.rows.lazy.filter {
                $0.action?.isInFlight == true
            }.count
            let actionIdentityCounts = latestSnapshot.rows.reduce(into: [String: Int]()) {
                guard let identity = $1.action?.identity else { return }
                $0[identity.rawValue, default: 0] += 1
            }
            let appliedSameIDOverlayCount = latestSnapshot.rows.lazy.filter {
                $0.representedPackageIDs.contains(overlayPackage.id)
                    && $0.detail?.contains(overlaySummary) == true
            }.count
            let requiredActions: [LibraryTableActionIdentity] = [.install, .unpin, .upgrade]
            let mixedActionsPassed = requiredActions.allSatisfy { actionIdentityCounts[$0.rawValue, default: 0] > 0 }
            let retainedRowsPassed = tableResult.maximumRetainedRowViewCount
                <= LibraryTableBenchmarkHarness.retainedRowViewBudget
            let passed = resultCount == rowCount
                && projectedRowCount == rowCount
                && actionRowCount == rowCount
                && inFlightRowCount > 0
                && mixedActionsPassed
                && appliedSameIDOverlayCount == 1
                && coldResult.projectionCacheMisses == iterations
                && warmProjectionCacheHits == iterations
                && retainedRowsPassed
                && coldIndexedSummary.p95Milliseconds <= 150
                && coldSnapshotSummary.p95Milliseconds <= LibraryTableBenchmarkHarness.coldSnapshotApplyP95Budget
                && coldEndToEndSummary.p95Milliseconds <= LibraryTableBenchmarkHarness.coldEndToEndP95Budget
                && warmIndexedSummary.medianMilliseconds <= 50
                && warmIndexedSummary.p95Milliseconds <= 100
                && warmEndToEndSummary.p95Milliseconds <= 100
                && framePassRatio >= 0.95
                && frameSummary.worstMilliseconds <= 50
            let payload: [String: Any] = [
                "passed": passed,
                "iterations": iterations,
                "sources": sourceCount,
                "rows": rowCount,
                "query": query,
                "timed_path": "query_shared_production_projection_cache_to_first_visible_cell",
                "result_count": resultCount,
                "projected_row_count": projectedRowCount,
                "action_row_count": actionRowCount,
                "action_identity_counts": actionIdentityCounts,
                "mixed_actions_passed": mixedActionsPassed,
                "in_flight_row_count": inFlightRowCount,
                "applied_same_id_overlay_count": appliedSameIDOverlayCount,
                "cold_projection_cache_misses": coldResult.projectionCacheMisses,
                "warm_projection_cache_hits": warmProjectionCacheHits,
                "cold_indexed_derivation_ms": libraryBenchmarkSummary(coldIndexedSummary, samples: coldResult.indexedDerivationSamples),
                "cold_snapshot_apply_ms": libraryBenchmarkSummary(coldSnapshotSummary, samples: coldResult.snapshotApplySamples),
                "cold_end_to_end_ms": libraryBenchmarkSummary(coldEndToEndSummary, samples: coldResult.endToEndSamples),
                "warm_indexed_derivation_ms": libraryBenchmarkSummary(warmIndexedSummary, samples: warmIndexedDerivationSamples),
                "warm_production_end_to_end_ms": libraryBenchmarkSummary(warmEndToEndSummary, samples: warmEndToEndSamples),
                "scroll_frame_ms": [
                    "median": frameSummary.medianMilliseconds,
                    "p95": frameSummary.p95Milliseconds,
                    "worst": frameSummary.worstMilliseconds,
                    "samples": tableResult.frameSamples,
                    "within_16_7_ms": tableResult.framesWithinSixteenMilliseconds,
                    "sample_count": tableResult.frameSamples.count,
                    "pass_ratio": framePassRatio,
                    "realized_row_views": tableResult.maximumRetainedRowViewCount,
                    "retained_row_view_budget": LibraryTableBenchmarkHarness.retainedRowViewBudget,
                    "retained_row_views_passed": retainedRowsPassed,
                ],
                "budgets_ms": [
                    "cold_indexed_derivation_p95": 150,
                    "cold_snapshot_apply_p95": LibraryTableBenchmarkHarness
                        .coldSnapshotApplyP95Budget,
                    "cold_end_to_end_p95": LibraryTableBenchmarkHarness
                        .coldEndToEndP95Budget,
                    "warm_indexed_derivation_median": 50,
                    "warm_indexed_derivation_p95": 100,
                    "warm_production_end_to_end_p95": 100,
                    "scroll_frame_p95_target": 16.7,
                    "scroll_frame_worst": 50,
                ],
            ]
            if let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys]),
               let json = String(data: data, encoding: .utf8) {
                FileHandle.standardOutput.write(
                    Data("HELM_LIBRARY_BENCHMARK \(json)\n".utf8)
                )
            }
            NSApp.terminate(nil)
        }
        return true
    }

    private func libraryBenchmarkSummary(
        _ summary: LibraryPerformanceSampleSummary,
        samples: [Double]
    ) -> [String: Any] {
        [
            "median": summary.medianMilliseconds,
            "p95": summary.p95Milliseconds,
            "worst": summary.worstMilliseconds,
            "samples": samples,
        ]
    }

    private func libraryBenchmarkProjectionFixture(
        packages: [PackageItem],
        sourceCount: Int
    ) -> LibraryBenchmarkProjectionFixture {
        let actionLabels = LibraryTableActionLabels()
        let inFlightPackages = packages.indices
            .filter { $0.isMultiple(of: 17) }
            .map { packages[$0] }
        let pinActionPackageIDs = Set(
            inFlightPackages.filter(\.pinned).map(\.id)
        )
        let upgradeActionPackageIDs = Set(
            inFlightPackages.filter {
                !$0.pinned && $0.status != .available
            }.map(\.id)
        )
        let installActionPackageNames = Set(
            inFlightPackages.filter { $0.status == .available }.map {
                PackageActionTracking.normalizedPackageIdentityKey(
                    name: $0.name,
                    version: $0.version
                )
            }
        )
        let installablePackageNames = Set(
            packages.filter { $0.status == .available }.map {
                PackageActionTracking.normalizedPackageIdentityKey(
                    name: $0.name,
                    version: $0.version
                )
            }
        )
        let managerCapabilities = (0..<sourceCount).map { index in
            ProductionLibraryManagerCapability(
                managerID: "manager-\(index)",
                enabled: true,
                supportsPackageInstall: true,
                supportsPackageUpgrade: true,
                supportsRemoteSearch: true,
                isUninstalling: false
            )
        }
        let identity: (Int) -> ProductionLibraryTableProjectionIdentity = { sourceFingerprint in
            ProductionLibraryTableProjectionIdentity(
                sourceFingerprint: sourceFingerprint,
                managerConstraint: nil,
                selectedPackageID: nil,
                managerPreferences: [:],
                pinActionPackageIDs: pinActionPackageIDs,
                upgradeActionPackageIDs: upgradeActionPackageIDs,
                installablePackageNames: installablePackageNames,
                installActionPackageNames: installActionPackageNames,
                managerCapabilities: managerCapabilities,
                appUpdateCapability: ProductionLibraryAppUpdateCapability(
                    updateAvailable: false,
                    canCheckForUpdates: false
                ),
                networkOperationsAvailable: true,
                locale: LocalizationManager.shared.currentLocale
            )
        }
        let action: (ConsolidatedPackageItem, PackageItem) -> LibraryTableAction? = { _, package in
            let identity: LibraryTableActionIdentity
            let symbolName: String
            let title: String
            let canPerform: Bool
            let isInFlight: Bool
            if package.pinned {
                (identity, symbolName, title, canPerform, isInFlight) = (
                    .unpin,
                    "pin.slash",
                    actionLabels.unpin,
                    true,
                    pinActionPackageIDs.contains(package.id)
                )
            } else if package.status == .available {
                let packageName = PackageActionTracking.normalizedPackageIdentityKey(
                    name: package.name,
                    version: package.version
                )
                (identity, symbolName, title, canPerform, isInFlight) = (
                    .install,
                    "arrow.down.circle",
                    actionLabels.install,
                    installablePackageNames.contains(packageName),
                    installActionPackageNames.contains(packageName)
                )
            } else {
                (identity, symbolName, title, canPerform, isInFlight) = (
                    .upgrade,
                    "arrow.up.circle",
                    actionLabels.upgrade,
                    package.status == .upgradable,
                    upgradeActionPackageIDs.contains(package.id)
                )
            }
            return LibraryTableAction(
                identity: identity,
                symbolName: symbolName,
                title: title,
                isEnabled: canPerform && !isInFlight,
                isInFlight: isInFlight
            )
        }
        return LibraryBenchmarkProjectionFixture(action: action, identity: identity)
    }

    private func runColdLibraryBenchmark(
        iterations: Int,
        query: String,
        core: HelmCore,
        harness: LibraryTableBenchmarkHarness,
        projectionFixture: LibraryBenchmarkProjectionFixture
    ) -> ColdLibraryBenchmarkResult {
        var indexedSamples: [Double] = []
        var snapshotSamples: [Double] = []
        var endToEndSamples: [Double] = []
        var resultCount = 0
        var projectionCacheMisses = 0
        var projectionCache = ProductionLibraryTableProjectionCache(
            namespace: "benchmark-library"
        )

        for _ in 1...iterations {
            core.cachedLibraryPackageIndex = nil
            projectionCache.invalidate()
            let endToEndStartedAt = ProcessInfo.processInfo.systemUptime
            let results = core.filteredPackages(
                query: query,
                managerId: nil,
                statusFilter: nil
            )
            let snapshotStartedAt = ProcessInfo.processInfo.systemUptime
            indexedSamples.append((snapshotStartedAt - endToEndStartedAt) * 1_000)
            let sourceFingerprint = LibraryTableRowProjector.sourceFingerprint(for: results)
            let projection = projectionCache.resolve(
                identity: projectionFixture.identity(sourceFingerprint)
            ) { revision in
                LibraryTableRowProjector.project(
                    revision: revision,
                    packageRows: results,
                    selectedPackageID: nil,
                    managerConstraint: nil,
                    preferredManagerID: { _ in nil },
                    action: projectionFixture.action
                )
            }
            if !projection.cacheHit {
                projectionCacheMisses += 1
            }
            harness.apply(snapshot: projection.snapshot)
            let completedAt = ProcessInfo.processInfo.systemUptime
            snapshotSamples.append((completedAt - snapshotStartedAt) * 1_000)
            endToEndSamples.append((completedAt - endToEndStartedAt) * 1_000)
            resultCount = results.count
        }
        return ColdLibraryBenchmarkResult(
            indexedDerivationSamples: indexedSamples,
            snapshotApplySamples: snapshotSamples,
            endToEndSamples: endToEndSamples,
            resultCount: resultCount,
            projectionCacheMisses: projectionCacheMisses,
            projectionCache: projectionCache
        )
    }

    private func libraryBenchmarkPackages(
        rowCount: Int,
        sourceCount: Int
    ) -> [PackageItem] {
        (0..<rowCount).map { index in
            let status: PackageStatus = switch index % 4 {
            case 0: .available
            case 1: .upgradable
            default: .installed
            }
            return PackageItem(
                id: "manager-\(index % sourceCount):package-\(index)",
                name: "package-\(index)",
                version: "1.0.0",
                latestVersion: status == .upgradable ? "2.0.0" : nil,
                managerId: "manager-\(index % sourceCount)",
                manager: "Manager \(index % sourceCount)",
                summary: index == 0 ? nil : "Package \(index) summary marker",
                pinned: index % 4 == 2,
                restartRequired: index % 19 == 0,
                runtimeState: PackageRuntimeState(
                    isActive: index % 7 == 0,
                    isDefault: index % 11 == 0,
                    hasOverride: index % 13 == 0
                ),
                status: status
            )
        }
    }

    private func runPopoverOpeningBenchmarkIfRequested() {
        guard let iterations = WayfinderPopoverBenchmarkConfiguration.iterations(),
              WayfinderPopoverFixtureProvider.isActive() else {
            return
        }

        Task { @MainActor [weak self] in
            guard let self else { return }
            var acknowledgementSamples: [Double] = []
            var firstUsefulRenderSamples: [Double] = []

            for index in 0..<iterations {
                closePanel()
                await Task.yield()

                let startedAt = ProcessInfo.processInfo.systemUptime
                var acknowledgedAt = startedAt
                showPanel(allowWhileControlCenterVisible: true) {
                    acknowledgedAt = ProcessInfo.processInfo.systemUptime
                }
                panel.contentView?.layoutSubtreeIfNeeded()
                panel.displayIfNeeded()
                let renderedAt = ProcessInfo.processInfo.systemUptime

                acknowledgementSamples.append((acknowledgedAt - startedAt) * 1_000)
                firstUsefulRenderSamples.append((renderedAt - startedAt) * 1_000)
                closePanel()

                if index + 1 < iterations {
                    try? await Task.sleep(nanoseconds: 25_000_000)
                }
            }

            let acknowledgementJSON = benchmarkJSON(for: acknowledgementSamples)
            let renderJSON = benchmarkJSON(for: firstUsefulRenderSamples)
            let payload = "HELM_WAYFINDER_POPOVER_BENCHMARK "
                + "{\"iterations\":\(iterations),"
                + "\"acknowledgement_ms\":\(acknowledgementJSON),"
                + "\"first_useful_render_ms\":\(renderJSON)}\n"
            FileHandle.standardOutput.write(Data(payload.utf8))
            NSApp.terminate(nil)
        }
    }

    private func benchmarkJSON(for samples: [Double]) -> String {
        let values = samples.map {
            String(format: "%.3f", locale: Locale(identifier: "en_US_POSIX"), $0)
        }
        return "[\(values.joined(separator: ","))]"
    }
    #endif

    private func closePanel() {
        panel.orderOut(nil)
        eventMonitor?.stop()
        core.setInteractiveSurfaceVisibility(popoverVisible: false, controlCenterVisible: isControlCenterVisible)
    }

    private func bindStatusItem() {
        core.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                DispatchQueue.main.async {
                    self?.updateStatusItemAppearance()
                    self?.resizePopoverIfVisible()
                }
            }
            .store(in: &cancellables)

        core.overviewState.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                DispatchQueue.main.async {
                    self?.updateStatusItemAppearance()
                }
            }
            .store(in: &cancellables)

        LocalizationManager.shared.$currentLocale
            .dropFirst()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.registerUserNotificationCategories()
            }
            .store(in: &cancellables)

        core.$taskTimeoutPrompts
            .receive(on: RunLoop.main)
            .sink { [weak self] prompts in
                self?.handleTaskTimeoutPrompts(prompts)
            }
            .store(in: &cancellables)

        core.$notificationsEnabled
            .dropFirst()
            .receive(on: RunLoop.main)
            .sink { [weak self] enabled in
                self?.handleNotificationsEnabledChanged(enabled)
            }
            .store(in: &cancellables)

        Publishers.CombineLatest4(
            core.$outdatedPackages,
            core.$managerStatuses,
            core.$safeModeEnabled,
            AppUpdateCoordinator.shared.$includeHelmInUpgradeAll
        )
            .debounce(for: .milliseconds(750), scheduler: RunLoop.main)
            .sink { [weak self] packages, _, _, _ in
                self?.handleUpdateAvailabilityChanged(packages)
            }
            .store(in: &cancellables)

        core.$upgradePlanCompletion
            .compactMap { $0 }
            .receive(on: RunLoop.main)
            .sink { [weak self] completion in
                self?.handleUpgradePlanCompletion(completion)
            }
            .store(in: &cancellables)
    }

    private func resizePopoverIfVisible() {
        guard panel.isVisible else { return }
        let panelWidth = WayfinderPopoverLayout.width
        let targetSize = NSSize(
            width: panelWidth,
            height: preferredPopoverHeight(forWidth: panelWidth)
        )
        let currentSize = panel.contentView?.frame.size ?? panel.contentLayoutRect.size
        guard abs(currentSize.width - targetSize.width) > 0.5
                || abs(currentSize.height - targetSize.height) > 0.5 else {
            return
        }
        panel.setContentSize(targetSize)
    }

    private func preferredPopoverHeight(forWidth _: CGFloat) -> CGFloat {
        core.hasCompletedOnboarding && !core.requiresLicenseTermsAcceptance
            ? WayfinderPopoverLayout.ordinaryHeight
            : WayfinderPopoverLayout.onboardingHeight
    }

    private func updateStatusItemAppearance() {
        guard let button = statusItem?.button else { return }

        let projection = core.overviewState.wayfinderProjection.content

        let anchorTint = menuBaseTint(for: button)
        let badge: StatusBadge?
        switch projection.condition {
        case .approvalRequired, .actionableFinding:
            badge = .symbol("!", .helmNeedsReview)
        case .failedOrInterrupted:
            badge = .symbol("!", .systemRed)
        case .activeWork, .refreshing:
            badge = .dot(.systemBlue)
        case let .updatesReady(count):
            badge = .count(min(99, count), .helmUpdatesReady)
        case .offline, .serviceUnavailable:
            badge = .symbol("-", .systemGray)
        case .healthy:
            badge = nil
        }
        button.image = statusItemImage(anchorTint: anchorTint, button: button, badge: badge)
        button.contentTintColor = nil
        button.title = ""
        button.imagePosition = .imageOnly

        let statusDescription = projection.title.localized
        button.toolTip = statusDescription
        button.setAccessibilityLabel(statusDescription)
    }

    private func statusItemImage(anchorTint: NSColor, button: NSStatusBarButton, badge: StatusBadge?) -> NSImage? {
        let baseImage: NSImage
        if let menuIcon = NSImage(named: "MenuBarIcon")?.copy() as? NSImage {
            baseImage = menuIcon
        } else if let fallback = NSImage(systemSymbolName: "anchor.fill", accessibilityDescription: "Helm") {
            baseImage = fallback
        } else {
            return nil
        }

        let targetSize = button.bounds.size.width > 0
            ? NSSize(width: 18, height: 18)
            : baseImage.size

        let rendered = NSImage(size: targetSize)
        rendered.lockFocus()
        let bounds = NSRect(origin: .zero, size: targetSize)
        let iconRect = bounds.insetBy(dx: 1, dy: 1)
        baseImage.draw(in: iconRect, from: .zero, operation: .sourceOver, fraction: 1.0)

        if let badge {
            // Manual tint needed when compositing colored badges
            anchorTint.set()
            iconRect.fill(using: .sourceAtop)
            drawBadge(badge, in: bounds)
        }
        rendered.unlockFocus()
        rendered.isTemplate = badge == nil
        return rendered
    }

    private func menuBaseTint(for button: NSStatusBarButton) -> NSColor {
        var color = NSColor.labelColor
        button.effectiveAppearance.performAsCurrentDrawingAppearance {
            color = NSColor.labelColor.usingColorSpace(.sRGB) ?? NSColor.labelColor
        }
        return color
    }

    private func statusItemButtonFrame() -> NSRect? {
        guard let button = statusItem?.button else { return nil }
        return button.window?.convertToScreen(button.frame)
    }

    private func focusControlCenterSearch() {
        openControlCenter()
        controlCenterContext.isControlCenterSearchPresented = true
        if #unavailable(macOS 26.0) {
            controlCenterContext.controlCenterSearchFocusRouter.requestFocus()
        }
    }

    private func handlePopoverEscape() {
        if panel.isVisible {
            closePanel()
        }
    }
}

// MARK: - Notifications

private extension AppDelegate {
    var shouldSuppressTaskNotifications: Bool {
        !core.notificationsEnabled || panel.isVisible || isControlCenterVisible
    }

    func configureUserNotifications() {
        notificationCenter.delegate = self
        registerUserNotificationCategories()

        guard core.notificationsEnabled else { return }
        requestNotificationAuthorization()
    }

    func registerUserNotificationCategories() {
        let waitAction = UNNotificationAction(
            identifier: Self.timeoutPromptActionWaitId,
            title: L10n.App.Tasks.Notification.timeoutPromptActionWait.localized,
            options: []
        )
        let stopAction = UNNotificationAction(
            identifier: Self.timeoutPromptActionStopId,
            title: L10n.App.Tasks.Notification.timeoutPromptActionStop.localized,
            options: [.destructive]
        )
        let timeoutPromptCategory = UNNotificationCategory(
            identifier: Self.timeoutPromptCategoryId,
            actions: [waitAction, stopAction],
            intentIdentifiers: [],
            options: [.customDismissAction]
        )
        let reviewPlanAction = UNNotificationAction(
            identifier: Self.reviewPlanActionId,
            title: L10n.App.Updates.Notification.reviewPlan.localized,
            options: [.foreground]
        )
        let upgradeAllAction = UNNotificationAction(
            identifier: Self.upgradeAllActionId,
            title: L10n.App.Updates.Notification.upgradeAll.localized,
            options: [.foreground]
        )
        let upgradePlanCompletionCategory = UNNotificationCategory(
            identifier: Self.upgradePlanCompletionCategoryId,
            actions: [reviewPlanAction],
            intentIdentifiers: [],
            options: []
        )
        let updatesAvailableReviewCategory = UNNotificationCategory(
            identifier: Self.updatesAvailableReviewCategoryId,
            actions: [reviewPlanAction],
            intentIdentifiers: [],
            options: []
        )
        let updatesAvailableUpgradeCategory = UNNotificationCategory(
            identifier: Self.updatesAvailableUpgradeCategoryId,
            actions: [reviewPlanAction, upgradeAllAction],
            intentIdentifiers: [],
            options: []
        )
        notificationCenter.setNotificationCategories([
            timeoutPromptCategory,
            upgradePlanCompletionCategory,
            updatesAvailableReviewCategory,
            updatesAvailableUpgradeCategory,
        ])
    }

    func requestNotificationAuthorization() {
        notificationCenter.requestAuthorization(options: [.alert, .sound]) { granted, error in
            if let error {
                appDelegateLogger.warning("notification authorization request failed: \(error.localizedDescription, privacy: .public)")
                return
            }
            appDelegateLogger.info("notification authorization granted=\(granted, privacy: .public)")
        }
    }

    func handleNotificationsEnabledChanged(_ enabled: Bool) {
        guard enabled else {
            announcedTimeoutPromptIds = []
            announcedUpgradePlanCompletionWorkflowId = nil
            notificationCenter.removeAllPendingNotificationRequests()
            notificationCenter.removeAllDeliveredNotifications()
            return
        }
        requestNotificationAuthorization()
    }

    func handleTaskTimeoutPrompts(_ prompts: [CoreTaskTimeoutPrompt]) {
        let activePromptIds = Set(prompts.map(\.id))
        announcedTimeoutPromptIds.formIntersection(activePromptIds)

        guard !shouldSuppressTaskNotifications else { return }
        for prompt in prompts {
            guard !announcedTimeoutPromptIds.contains(prompt.id) else { continue }
            postTaskTimeoutPromptNotification(prompt)
            announcedTimeoutPromptIds.insert(prompt.id)
        }
    }

    func handleUpgradePlanCompletion(_ completion: UpgradePlanCompletion) {
        guard completion.completedNormally,
              announcedUpgradePlanCompletionWorkflowId != completion.workflowId else {
            return
        }
        announcedUpgradePlanCompletionWorkflowId = completion.workflowId
        guard !shouldSuppressTaskNotifications else { return }
        guard completion.remainingInteractiveCount > 0 else { return }
        postUpgradePlanCompletionNotification(completion)
    }

    func handleUpdateAvailabilityChanged(_ packages: [PackageItem]) {
        let interactiveSurfaceVisible = panel.isVisible || isControlCenterVisible
        let automaticUpdateCount = core.upgradeAllPreviewCount(
            includePinned: false,
            allowOsUpdates: false
        )
        var updateIdentifiers = packages.map { package in
            [
                package.managerId,
                package.id,
                package.version,
                package.latestVersion ?? "",
            ].joined(separator: "|")
        }
        if !packages.isEmpty {
            updateIdentifiers.append("upgrade-all-available:\(automaticUpdateCount > 0)")
        }
        let evaluation = AppUpdateNotificationPolicy.evaluate(
            updateIdentifiers: updateIdentifiers,
            previousFingerprint: observedUpdateFingerprint,
            notificationsEnabled: core.notificationsEnabled,
            interactiveSurfaceVisible: interactiveSurfaceVisible
        )
        observedUpdateFingerprint = evaluation.observedFingerprint

        if evaluation.observedFingerprint == nil || interactiveSurfaceVisible {
            notificationCenter.removePendingNotificationRequests(
                withIdentifiers: [Self.updatesAvailableNotificationId]
            )
            notificationCenter.removeDeliveredNotifications(
                withIdentifiers: [Self.updatesAvailableNotificationId]
            )
        }

        guard evaluation.shouldNotify else { return }
        postUpdatesReadyNotification(
            count: packages.count,
            allowsUpgradeAll: automaticUpdateCount > 0
        )
    }

    func postTaskTimeoutPromptNotification(_ prompt: CoreTaskTimeoutPrompt) {
        let managerName = core.normalizedManagerName(prompt.manager)
        let content = UNMutableNotificationContent()
        content.title = L10n.App.Tasks.Notification.timeoutPromptTitle.localized(
            with: ["manager": managerName]
        )
        content.body = L10n.App.Tasks.Notification.timeoutPromptMessage.localized(
            with: [
                "manager": managerName,
                "grace_seconds": Int(prompt.graceSeconds),
                "extension_seconds": Int(prompt.suggestedExtensionSeconds)
            ]
        )
        content.sound = .default
        content.categoryIdentifier = Self.timeoutPromptCategoryId
        content.userInfo = [
            Self.timeoutPromptTaskIdUserInfoKey: NSNumber(value: Int64(prompt.taskId)),
            Self.timeoutPromptIdUserInfoKey: prompt.id
        ]

        let request = UNNotificationRequest(
            identifier: "helm.task.timeout.\(prompt.id)",
            content: content,
            trigger: nil
        )
        notificationCenter.add(request) { error in
            if let error {
                appDelegateLogger.warning(
                    "failed to post timeout prompt notification for task \(prompt.taskId): \(error.localizedDescription, privacy: .public)"
                )
            }
        }
    }

    func postUpdatesReadyNotification(count: Int, allowsUpgradeAll: Bool) {
        let content = UNMutableNotificationContent()
        content.title = L10n.App.Updates.Notification.readyTitle.localized(with: [
            "count": count
        ])
        content.body = L10n.App.Updates.Notification.readyMessage.localized
        content.sound = .default
        content.categoryIdentifier = allowsUpgradeAll
            ? Self.updatesAvailableUpgradeCategoryId
            : Self.updatesAvailableReviewCategoryId

        let request = UNNotificationRequest(
            identifier: Self.updatesAvailableNotificationId,
            content: content,
            trigger: nil
        )
        notificationCenter.add(request) { error in
            if let error {
                appDelegateLogger.warning(
                    "failed to post updates-ready notification: \(error.localizedDescription, privacy: .public)"
                )
            }
        }
    }

    func postUpgradePlanCompletionNotification(_ completion: UpgradePlanCompletion) {
        let content = UNMutableNotificationContent()
        content.title = L10n.App.Updates.Completion.title.localized
        content.body = L10n.App.Updates.Completion.message.localized(with: [
            "count": completion.remainingInteractiveCount
        ])
        content.sound = .default
        content.categoryIdentifier = Self.upgradePlanCompletionCategoryId

        let request = UNNotificationRequest(
            identifier: "helm.upgrade-plan.completed.\(completion.workflowId)",
            content: content,
            trigger: nil
        )
        notificationCenter.add(request) { error in
            if let error {
                appDelegateLogger.warning(
                    "failed to post upgrade-plan completion notification: \(error.localizedDescription, privacy: .public)"
                )
            }
        }
    }
}

// MARK: - Control Center & Status Menu

private extension AppDelegate {
    func openControlCenter() {
        closePanel()

        if controlCenterWindowController == nil {
            let isPresentingFirstRun = presentsEnvironmentBrief
            let initialSize = isPresentingFirstRun
                ? HelmPrimaryWindowSizingPolicy.firstRunSize
                : HelmPrimaryWindowSizingPolicy.dashboardDefaultSize
            let rootView = ControlCenterWindowView(onFirstRunComplete: { [weak self] in
                self?.transitionControlCenterFromFirstRun()
            })
                .environmentObject(controlCenterContext)

            let hostingController = NSHostingController(rootView: rootView)
            HelmHostingSizingPolicy.apply(to: hostingController)
            let window = ControlCenterWindow(
                contentRect: NSRect(origin: .zero, size: initialSize),
                styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                backing: .buffered,
                defer: false
            )
            window.onCommandF = { [weak self] in
                self?.focusControlCenterSearch()
            }
            window.onEscape = { [weak self] in
                self?.handlePopoverEscape()
            }
            window.title = "app.window.control_center".localized
            window.titleVisibility = HelmWindowChromePolicy.titleVisibility
            window.titlebarAppearsTransparent = true
            window.delegate = self
            window.contentViewController = hostingController
            window.autorecalculatesKeyViewLoop = true
            window.isReleasedWhenClosed = false

            if isPresentingFirstRun {
                HelmPrimaryWindowSizingPolicy.applyFirstRun(to: window)
                window.center()
            } else {
                HelmPrimaryWindowSizingPolicy.applyDashboard(to: window)
                let restoredFrame = window.setFrameUsingName(Self.controlCenterFrameAutosaveName)
                HelmPrimaryWindowSizingPolicy.applyDashboard(to: window)
                window.setFrameAutosaveName(Self.controlCenterFrameAutosaveName)
                if !restoredFrame {
                    window.center()
                }
            }

            controlCenterWindowController = NSWindowController(window: window)
        }

        guard let window = controlCenterWindowController?.window else { return }
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        core.setInteractiveSurfaceVisibility(popoverVisible: false, controlCenterVisible: true)
    }

    func transitionControlCenterFromFirstRun() {
        DispatchQueue.main.async { [weak self] in
            guard let self,
                  let window = controlCenterWindowController?.window else { return }

            HelmPrimaryWindowSizingPolicy.applyDashboard(to: window)
            let restoredFrame = window.setFrameUsingName(Self.controlCenterFrameAutosaveName)
            if !restoredFrame {
                window.setContentSize(HelmPrimaryWindowSizingPolicy.dashboardDefaultSize)
                window.center()
            }
            HelmPrimaryWindowSizingPolicy.applyDashboard(to: window)
            window.setFrameAutosaveName(Self.controlCenterFrameAutosaveName)
        }
    }

    @objc func handleSystemAppearanceChanged() {
        updateStatusItemAppearance()
    }
}

extension AppDelegate {
    func windowWillResize(_ sender: NSWindow, to frameSize: NSSize) -> NSSize {
        if sender == settingsWindowController?.window {
            return HelmPrimaryWindowSizingPolicy.settingsResizeSize(frameSize)
        }
        if sender == controlCenterWindowController?.window {
            return presentsEnvironmentBrief
                ? HelmPrimaryWindowSizingPolicy.firstRunSize
                : HelmPrimaryWindowSizingPolicy.dashboardResizeSize(frameSize)
        }
        return frameSize
    }

    func windowWillClose(_ notification: Notification) {
        guard let window = notification.object as? NSWindow else {
            return
        }
        if window == settingsWindowController?.window {
            window.parent?.removeChildWindow(window)
            return
        }
        guard window == controlCenterWindowController?.window else { return }
        HelmSettingsPanelPolicy.detachSettingsWindowFromClosingDashboard(
            settingsWindow: settingsWindowController?.window,
            dashboardWindow: window
        )
        core.setInteractiveSurfaceVisibility(popoverVisible: panel.isVisible, controlCenterVisible: false)
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler(core.notificationsEnabled ? [.banner, .sound] : [])
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        defer { completionHandler() }

        let userInfo = response.notification.request.content.userInfo
        let categoryIdentifier = response.notification.request.content.categoryIdentifier
        if categoryIdentifier == Self.upgradePlanCompletionCategoryId
            || categoryIdentifier == Self.updatesAvailableReviewCategoryId
            || categoryIdentifier == Self.updatesAvailableUpgradeCategoryId {
            let requestsPlanConfirmation = response.actionIdentifier == Self.upgradeAllActionId
            guard response.actionIdentifier == UNNotificationDefaultActionIdentifier
                    || response.actionIdentifier == Self.reviewPlanActionId
                    || requestsPlanConfirmation else {
                return
            }
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                if requestsPlanConfirmation,
                   core.upgradeAllPreviewCount(includePinned: false, allowOsUpdates: false) > 0 {
                    controlCenterContext.requestUpgradeAllPlanConfirmation()
                } else {
                    controlCenterContext.select(.updates)
                }
                openControlCenter()
            }
            return
        }

        if let promptId = userInfo[Self.timeoutPromptIdUserInfoKey] as? String {
            announcedTimeoutPromptIds.remove(promptId)
        }

        guard categoryIdentifier == Self.timeoutPromptCategoryId else {
            return
        }

        let taskId: UInt64? = {
            if let value = userInfo[Self.timeoutPromptTaskIdUserInfoKey] as? NSNumber {
                return value.uint64Value
            }
            if let value = userInfo[Self.timeoutPromptTaskIdUserInfoKey] as? String,
               let parsed = UInt64(value) {
                return parsed
            }
            return nil
        }()

        guard let taskId else { return }
        switch response.actionIdentifier {
        case Self.timeoutPromptActionWaitId:
            core.respondTaskTimeoutPrompt(taskId: taskId, waitForCompletion: true)
        case Self.timeoutPromptActionStopId:
            core.respondTaskTimeoutPrompt(taskId: taskId, waitForCompletion: false)
        default:
            break
        }
    }
}
