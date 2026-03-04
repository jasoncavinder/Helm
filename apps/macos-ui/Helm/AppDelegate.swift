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
    private var statusItem: NSStatusItem?
    private var panel: FloatingPanel!
    private var eventMonitor: EventMonitor?
    private var controlCenterWindowController: NSWindowController?
    private var statusMenu: NSMenu?
    private var aboutMenuItem: NSMenuItem?
    private var checkForUpdatesMenuItem: NSMenuItem?
    private var controlCenterMenuItem: NSMenuItem?
    private var settingsMenuItem: NSMenuItem?
    private var upgradeAllMenuItem: NSMenuItem?
    private var refreshMenuItem: NSMenuItem?
    private var cancellables: Set<AnyCancellable> = []

    private let core = HelmCore.shared
    private let appUpdate = AppUpdateCoordinator.shared
    private let controlCenterContext = ControlCenterContext()
    private let notificationCenter = UNUserNotificationCenter.current()
    private var hasObservedInFlightTasks = false
    private var announcedTimeoutPromptIds: Set<String> = []
    private static let timeoutPromptCategoryId = "helm.task.timeout.prompt"
    private static let timeoutPromptActionWaitId = "helm.task.timeout.prompt.wait"
    private static let timeoutPromptActionStopId = "helm.task.timeout.prompt.stop"
    private static let timeoutPromptTaskIdUserInfoKey = "task_id"
    private static let timeoutPromptIdUserInfoKey = "prompt_id"
    private var isControlCenterVisible: Bool {
        controlCenterWindowController?.window?.isVisible == true
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)

        let contentView = RedesignPopoverView(onOpenControlCenter: { [weak self] in
            self?.openControlCenter()
            self?.closePanel()
        })
        .environmentObject(controlCenterContext)
        .background(VisualEffect().ignoresSafeArea())
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))

        panel = FloatingPanel(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 620),
            backing: .buffered,
            defer: false
        )
        panel.onCommandF = { [weak self] in
            self?.focusPopoverSearch()
        }
        panel.onEscape = { [weak self] in
            self?.handlePopoverEscape()
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
        configureStatusMenu()
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

        if core.hasCompletedOnboarding && !core.requiresLicenseTermsAcceptance {
            core.triggerRefresh()
        }
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

    @objc private func togglePanel(_ sender: AnyObject?) {
        if NSApp.currentEvent?.type == .rightMouseUp {
            showStatusMenu()
            return
        }

        if isControlCenterVisible {
            openControlCenter()
            closePanel()
            return
        }

        if panel.isVisible {
            closePanel()
        } else {
            showPanel()
        }
    }

    private func showPanel() {
        guard statusItem?.button != nil else { return }
        guard !isControlCenterVisible else {
            openControlCenter()
            return
        }

        let buttonRect = statusItemButtonFrame() ?? .zero

        let panelWidth: CGFloat = 400
        panel.setContentSize(NSSize(width: panelWidth, height: preferredPopoverHeight(forWidth: panelWidth)))

        let panelSize = panel.frame.size
        let x = buttonRect.origin.x + (buttonRect.width / 2) - (panelSize.width / 2)
        let y = buttonRect.origin.y - panelSize.height - 6

        panel.setFrameOrigin(NSPoint(x: x, y: y))
        panel.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        eventMonitor?.start()
        core.setInteractiveSurfaceVisibility(popoverVisible: true, controlCenterVisible: isControlCenterVisible)
    }

    private func closePanel() {
        panel.orderOut(nil)
        eventMonitor?.stop()
        core.setInteractiveSurfaceVisibility(popoverVisible: false, controlCenterVisible: isControlCenterVisible)
    }

    private func bindStatusItem() {
        core.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.updateStatusItemAppearance()
                self?.updateStatusMenuState()
                self?.resizePopoverIfVisible()
            }
            .store(in: &cancellables)

        appUpdate.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.updateStatusMenuState()
            }
            .store(in: &cancellables)

        controlCenterContext.$suppressWindowBackgroundDragging
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.updateControlCenterWindowDragBehavior()
            }
            .store(in: &cancellables)

        controlCenterContext.$selectedSection
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.updateControlCenterWindowDragBehavior()
            }
            .store(in: &cancellables)

        core.$taskTimeoutPrompts
            .receive(on: RunLoop.main)
            .sink { [weak self] prompts in
                self?.handleTaskTimeoutPrompts(prompts)
            }
            .store(in: &cancellables)

        core.$activeTasks
            .receive(on: RunLoop.main)
            .sink { [weak self] tasks in
                self?.handleActiveTasksUpdated(tasks)
            }
            .store(in: &cancellables)
    }

    private func resizePopoverIfVisible() {
        guard panel.isVisible else { return }
        let panelWidth = panel.frame.width > 0 ? panel.frame.width : 400
        panel.setContentSize(NSSize(width: panelWidth, height: preferredPopoverHeight(forWidth: panelWidth)))
    }

    private func preferredPopoverHeight(forWidth width: CGFloat) -> CGFloat {
        guard let view = panel.contentViewController?.view else { return 620 }
        let oldFrame = view.frame
        view.frame = NSRect(x: 0, y: 0, width: width, height: oldFrame.height)
        view.layoutSubtreeIfNeeded()
        let measured = ceil(view.fittingSize.height)
        view.frame = oldFrame
        return max(520, min(740, measured))
    }

    private func updateStatusItemAppearance() {
        guard let button = statusItem?.button else { return }

        let outdatedCount = core.outdatedPackages.count
        let failedTaskCount = core.failedTaskCount
        let running = core.runningTaskCount > 0 || core.isRefreshing

        let anchorTint = menuBaseTint(for: button)
        let badge: StatusBadge?
        if failedTaskCount > 0 {
            badge = .symbol("!", .systemRed)
        } else if outdatedCount > 0 {
            badge = .count(min(99, outdatedCount), .systemOrange)
        } else if running {
            badge = .dot(.systemBlue)
        } else {
            badge = nil
        }
        button.image = statusItemImage(anchorTint: anchorTint, button: button, badge: badge)
        button.contentTintColor = nil
        button.title = ""
        button.imagePosition = .imageOnly

        let statusDescription: String
        if failedTaskCount > 0 {
            statusDescription = "app.status_item.error".localized(with: ["count": failedTaskCount])
        } else if outdatedCount > 0 {
            statusDescription = "app.status_item.updates".localized(with: ["count": outdatedCount])
        } else if running {
            statusDescription = "app.status_item.running".localized
        } else {
            statusDescription = "app.status_item.healthy".localized
        }
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

    private func openPopoverOverlay(_ route: PopoverOverlayRoute) {
        guard !isControlCenterVisible else {
            openControlCenter()
            return
        }
        showPanel()
        controlCenterContext.popoverOverlayRequest = nil
        DispatchQueue.main.async { [weak self] in
            self?.controlCenterContext.popoverOverlayRequest = route
        }
    }

    private func focusPopoverSearch() {
        openPopoverOverlay(.search)
        controlCenterContext.popoverSearchFocusToken += 1
    }

    private func focusControlCenterSearch() {
        openControlCenter()
        controlCenterContext.controlCenterSearchFocusToken += 1
    }

    private func handlePopoverEscape() {
        if controlCenterContext.isPopoverOverlayVisible {
            controlCenterContext.popoverOverlayDismissToken += 1
        } else if panel.isVisible {
            closePanel()
        }
    }
}

// MARK: - Notifications

private extension AppDelegate {
    var shouldSuppressTaskNotifications: Bool {
        panel.isVisible || isControlCenterVisible
    }

    func configureUserNotifications() {
        notificationCenter.delegate = self

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
        notificationCenter.setNotificationCategories([timeoutPromptCategory])

        notificationCenter.requestAuthorization(options: [.alert, .sound]) { granted, error in
            if let error {
                appDelegateLogger.warning("notification authorization request failed: \(error.localizedDescription, privacy: .public)")
                return
            }
            appDelegateLogger.info("notification authorization granted=\(granted, privacy: .public)")
        }
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

    func handleActiveTasksUpdated(_ tasks: [TaskItem]) {
        let hasInFlight = tasks.contains(where: \.isRunning)
        if hasInFlight {
            hasObservedInFlightTasks = true
            return
        }
        guard hasObservedInFlightTasks else { return }
        hasObservedInFlightTasks = false

        guard !shouldSuppressTaskNotifications else { return }
        postTasksCompletedNotification()
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

    func postTasksCompletedNotification() {
        let content = UNMutableNotificationContent()
        content.title = L10n.App.Tasks.Notification.allCompleteTitle.localized
        content.body = L10n.App.Tasks.Notification.allCompleteMessage.localized
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: "helm.tasks.completed.\(Int(Date().timeIntervalSince1970))",
            content: content,
            trigger: nil
        )
        notificationCenter.add(request) { error in
            if let error {
                appDelegateLogger.warning(
                    "failed to post tasks-completed notification: \(error.localizedDescription, privacy: .public)"
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
            let rootView = ControlCenterWindowView()
                .environmentObject(controlCenterContext)

            let hostingController = NSHostingController(rootView: rootView)
            let window = ControlCenterWindow(
                contentRect: NSRect(x: 0, y: 0, width: 1120, height: 740),
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
            window.titleVisibility = .hidden
            window.titlebarAppearsTransparent = true
            window.isMovableByWindowBackground = shouldAllowControlCenterWindowBackgroundDragging()
            window.delegate = self
            if #available(macOS 11.0, *) {
                window.toolbarStyle = .unifiedCompact
            }
            if #available(macOS 11.0, *) {
                window.titlebarSeparatorStyle = .none
            }
            window.contentViewController = hostingController
            window.autorecalculatesKeyViewLoop = true
            window.isReleasedWhenClosed = false
            window.standardWindowButton(.miniaturizeButton)?.isEnabled = false
            window.standardWindowButton(.zoomButton)?.isEnabled = false
            let fixedSize = NSSize(width: 1120, height: 740)
            window.minSize = fixedSize
            window.maxSize = fixedSize
            window.center()

            controlCenterWindowController = NSWindowController(window: window)
        }

        guard let window = controlCenterWindowController?.window else { return }
        updateControlCenterWindowDragBehavior()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        core.setInteractiveSurfaceVisibility(popoverVisible: false, controlCenterVisible: true)
    }

    private func shouldAllowControlCenterWindowBackgroundDragging() -> Bool {
        // Interactive controls and manager drag/drop can suppress background dragging.
        return !controlCenterContext.suppressWindowBackgroundDragging
    }

    func updateControlCenterWindowDragBehavior() {
        controlCenterWindowController?.window?.isMovableByWindowBackground =
            shouldAllowControlCenterWindowBackgroundDragging()
    }

    func configureStatusMenu() {
        let menu = NSMenu()
        // We drive enablement explicitly from app/core state.
        // Cocoa auto-validation can otherwise re-enable items with actions.
        menu.autoenablesItems = false

        let aboutItem = NSMenuItem(
            title: "app.overlay.about.title".localized,
            action: #selector(openAboutFromMenu),
            keyEquivalent: ""
        )
        aboutItem.target = self
        menu.addItem(aboutItem)
        aboutMenuItem = aboutItem

        let checkForUpdatesItem = NSMenuItem(
            title: L10n.App.Overlay.About.checkForUpdates.localized,
            action: #selector(checkForUpdatesFromMenu),
            keyEquivalent: ""
        )
        checkForUpdatesItem.target = self
        menu.addItem(checkForUpdatesItem)
        checkForUpdatesMenuItem = checkForUpdatesItem

        let controlCenterItem = NSMenuItem(
            title: L10n.App.Action.openControlCenter.localized,
            action: #selector(openControlCenterFromMenu),
            keyEquivalent: ""
        )
        controlCenterItem.target = self
        menu.addItem(controlCenterItem)
        controlCenterMenuItem = controlCenterItem

        menu.addItem(.separator())

        let upgradeItem = NSMenuItem(
            title: L10n.App.Settings.Action.upgradeAll.localized,
            action: #selector(openUpgradeAllFromMenu),
            keyEquivalent: ""
        )
        upgradeItem.target = self
        menu.addItem(upgradeItem)
        upgradeAllMenuItem = upgradeItem

        let settingsItem = NSMenuItem(title: L10n.Common.settings.localized, action: nil, keyEquivalent: "")
        let settingsMenu = NSMenu()
        let basicSettingsItem = NSMenuItem(
            title: "app.overlay.settings.title".localized,
            action: #selector(openQuickSettingsFromMenu),
            keyEquivalent: ""
        )
        basicSettingsItem.target = self
        settingsMenu.addItem(basicSettingsItem)

        let advancedSettingsItem = NSMenuItem(
            title: "app.overlay.settings.open_advanced".localized,
            action: #selector(openAdvancedSettingsFromMenu),
            keyEquivalent: ""
        )
        advancedSettingsItem.target = self
        settingsMenu.addItem(advancedSettingsItem)
        settingsItem.submenu = settingsMenu
        menu.addItem(settingsItem)
        settingsMenuItem = settingsItem

        menu.addItem(buildSupportMenuItem())

        menu.addItem(.separator())

        let refreshItem = NSMenuItem(
            title: L10n.Common.refresh.localized,
            action: #selector(refreshFromMenu),
            keyEquivalent: ""
        )
        refreshItem.target = self
        menu.addItem(refreshItem)
        refreshMenuItem = refreshItem

        menu.addItem(.separator())

        let quitItem = NSMenuItem(
            title: L10n.Common.quit.localized,
            action: #selector(quitFromMenu),
            keyEquivalent: ""
        )
        quitItem.target = self
        menu.addItem(quitItem)

        statusMenu = menu
        statusItem?.menu = nil
        statusItem?.button?.menu = nil
        updateStatusMenuState()
    }

    private func buildSupportMenuItem() -> NSMenuItem {
        let item = NSMenuItem(
            title: L10n.App.Settings.SupportFeedback.supportHelm.localized,
            action: nil,
            keyEquivalent: ""
        )
        let submenu = NSMenu()
        for channel in SupportHelmChannel.allCases {
            let supportItem = NSMenuItem(
                title: channel.title,
                action: #selector(openSupportChannelFromMenu(_:)),
                keyEquivalent: ""
            )
            supportItem.target = self
            supportItem.representedObject = channel.url
            supportItem.isEnabled = channel.url != nil
            submenu.addItem(supportItem)
        }
        item.submenu = submenu
        return item
    }

    func updateStatusMenuState() {
        let onboardingComplete = core.hasCompletedOnboarding

        aboutMenuItem?.isEnabled = onboardingComplete
        controlCenterMenuItem?.isEnabled = onboardingComplete
        settingsMenuItem?.isEnabled = onboardingComplete

        checkForUpdatesMenuItem?.isEnabled = onboardingComplete
            && appUpdate.canCheckForUpdates
            && !appUpdate.isCheckingForUpdates
        checkForUpdatesMenuItem?.toolTip = onboardingComplete
            ? appUpdate.unavailableReasonLocalizationKey?.localized
            : nil
        upgradeAllMenuItem?.isEnabled = onboardingComplete && !core.outdatedPackages.isEmpty
        refreshMenuItem?.isEnabled = onboardingComplete && !core.isRefreshing
    }

    func showStatusMenu() {
        closePanel()
        updateStatusMenuState()
        if let statusItem, let menu = statusMenu {
            let previousMenu = statusItem.menu
            statusItem.menu = menu
            statusItem.button?.performClick(nil)
            statusItem.menu = previousMenu
        }
    }

    @objc func openAboutFromMenu() {
        openPopoverOverlay(.about)
    }

    @objc func checkForUpdatesFromMenu() {
        appUpdate.checkForUpdates()
    }

    @objc func openControlCenterFromMenu() {
        controlCenterContext.selectedSection = .overview
        openControlCenter()
    }

    @objc func openQuickSettingsFromMenu() {
        openPopoverOverlay(.quickSettings)
    }

    @objc func openAdvancedSettingsFromMenu() {
        controlCenterContext.selectedSection = .settings
        openControlCenter()
    }

    @objc func openUpgradeAllFromMenu() {
        controlCenterContext.selectedSection = .updates
        openControlCenter()
        controlCenterContext.presentUpgradeSheet(in: .controlCenter)
    }

    @objc func refreshFromMenu() {
        core.triggerRefresh()
    }

    @objc func quitFromMenu() {
        NSApplication.shared.terminate(nil)
    }

    @objc func openSupportChannelFromMenu(_ sender: NSMenuItem) {
        guard let url = sender.representedObject as? URL else { return }
        HelmSupport.openURL(url)
    }

    @objc func handleSystemAppearanceChanged() {
        updateStatusItemAppearance()
    }
}

extension AppDelegate {
    func windowWillClose(_ notification: Notification) {
        guard let window = notification.object as? NSWindow,
              window == controlCenterWindowController?.window else {
            return
        }
        core.setInteractiveSurfaceVisibility(popoverVisible: panel.isVisible, controlCenterVisible: false)
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        defer { completionHandler() }

        let userInfo = response.notification.request.content.userInfo
        if let promptId = userInfo[Self.timeoutPromptIdUserInfoKey] as? String {
            announcedTimeoutPromptIds.remove(promptId)
        }

        guard response.notification.request.content.categoryIdentifier == Self.timeoutPromptCategoryId else {
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
