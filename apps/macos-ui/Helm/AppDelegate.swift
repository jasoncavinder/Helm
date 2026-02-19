import Cocoa
import SwiftUI
import Combine

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var panel: FloatingPanel!
    private var eventMonitor: EventMonitor?
    private var controlCenterWindowController: NSWindowController?
    private var statusMenu: NSMenu?
    private var upgradeAllMenuItem: NSMenuItem?
    private var refreshMenuItem: NSMenuItem?
    private var cancellables: Set<AnyCancellable> = []

    private let core = HelmCore.shared
    private let controlCenterContext = ControlCenterContext()

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

        eventMonitor = EventMonitor(mask: [.leftMouseDown, .rightMouseDown]) { [weak self] event in
            guard let self else { return }
            guard self.panel.isVisible else { return }

            let clickPoint: NSPoint
            if let event, let sourceWindow = event.window {
                clickPoint = sourceWindow.convertPoint(toScreen: event.locationInWindow)
            } else {
                clickPoint = NSEvent.mouseLocation
            }

            let clickInPanel = self.panel.frame.contains(clickPoint)
            let clickInControlCenter = self.controlCenterWindowController?.window?.frame.contains(clickPoint) ?? false
            let clickInStatusItem = self.statusItemButtonFrame()?.contains(clickPoint) ?? false

            if !clickInPanel && !clickInStatusItem {
                self.closePanel()
            }
        }

        DistributedNotificationCenter.default().addObserver(
            self,
            selector: #selector(handleSystemAppearanceChanged),
            name: Notification.Name("AppleInterfaceThemeChangedNotification"),
            object: nil
        )

        if core.hasCompletedOnboarding {
            core.triggerRefresh()
        }
    }

    @objc private func togglePanel(_ sender: AnyObject?) {
        if NSApp.currentEvent?.type == .rightMouseUp {
            showStatusMenu()
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

        let buttonRect = statusItemButtonFrame() ?? .zero

        let panelWidth: CGFloat = 400
        panel.setContentSize(NSSize(width: panelWidth, height: preferredPopoverHeight(forWidth: panelWidth)))

        let panelSize = panel.frame.size
        let x = buttonRect.origin.x + (buttonRect.width / 2) - (panelSize.width / 2)
        let y = buttonRect.origin.y - panelSize.height - 6

        panel.setFrameOrigin(NSPoint(x: x, y: y))
        panel.makeKeyAndOrderFront(nil)
        eventMonitor?.start()
    }

    private func closePanel() {
        panel.orderOut(nil)
        eventMonitor?.stop()
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
        anchorTint.set()
        iconRect.fill(using: .sourceAtop)

        if let badge {
            drawBadge(badge, in: bounds)
        }
        rendered.unlockFocus()
        rendered.isTemplate = false
        return rendered
    }

    private func menuBaseTint(for button: NSStatusBarButton) -> NSColor {
        var color = NSColor.labelColor
        button.effectiveAppearance.performAsCurrentDrawingAppearance {
            color = NSColor.labelColor
        }
        return color
    }

    private func openControlCenter() {
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
            window.isMovableByWindowBackground = false
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
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func configureStatusMenu() {
        let menu = NSMenu()

        let aboutItem = NSMenuItem(
            title: "app.overlay.about.title".localized,
            action: #selector(openAboutFromMenu),
            keyEquivalent: ""
        )
        aboutItem.target = self
        menu.addItem(aboutItem)

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

    private func updateStatusMenuState() {
        upgradeAllMenuItem?.isEnabled = !core.outdatedPackages.isEmpty
        refreshMenuItem?.isEnabled = !core.isRefreshing
    }

    private func showStatusMenu() {
        closePanel()
        updateStatusMenuState()
        if let statusItem, let menu = statusMenu {
            let previousMenu = statusItem.menu
            statusItem.menu = menu
            statusItem.button?.performClick(nil)
            statusItem.menu = previousMenu
        }
    }

    private func statusItemButtonFrame() -> NSRect? {
        guard let button = statusItem?.button else { return nil }
        return button.window?.convertToScreen(button.frame)
    }

    private func openPopoverOverlay(_ route: PopoverOverlayRoute) {
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

    @objc private func openAboutFromMenu() {
        openPopoverOverlay(.about)
    }

    @objc private func openQuickSettingsFromMenu() {
        openPopoverOverlay(.quickSettings)
    }

    @objc private func openAdvancedSettingsFromMenu() {
        contextToSettingsSection()
        openControlCenter()
    }

    @objc private func openUpgradeAllFromMenu() {
        contextToUpdatesSection()
        openControlCenter()
        controlCenterContext.showUpgradeSheet = true
    }

    @objc private func refreshFromMenu() {
        core.triggerRefresh()
    }

    @objc private func quitFromMenu() {
        NSApplication.shared.terminate(nil)
    }

    @objc private func handleSystemAppearanceChanged() {
        updateStatusItemAppearance()
    }

    private func contextToSettingsSection() {
        controlCenterContext.selectedSection = .settings
    }

    private func contextToUpdatesSection() {
        controlCenterContext.selectedSection = .updates
    }
}

// MARK: - FloatingPanel

final class FloatingPanel: NSPanel {
    var onCommandF: (() -> Void)?
    var onEscape: (() -> Void)?

    init(contentRect: NSRect, backing: NSWindow.BackingStoreType, defer flag: Bool) {
        super.init(contentRect: contentRect, styleMask: [.nonactivatingPanel, .borderless], backing: backing, defer: flag)

        isFloatingPanel = true
        level = .mainMenu
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        backgroundColor = .clear
        isOpaque = false
        hasShadow = true
        isMovableByWindowBackground = false
        isReleasedWhenClosed = false
        hidesOnDeactivate = false
        // Enable Tab traversal for SwiftUI controls within this borderless panel
        autorecalculatesKeyViewLoop = true
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.type == .keyDown else { return super.performKeyEquivalent(with: event) }

        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let key = event.charactersIgnoringModifiers?.lowercased()

        if flags.contains(.command), key == "f" {
            onCommandF?()
            return true
        }

        if event.keyCode == 53 {
            onEscape?()
            return true
        }

        return super.performKeyEquivalent(with: event)
    }
}

final class ControlCenterWindow: NSWindow {
    var onCommandF: (() -> Void)?
    var onEscape: (() -> Void)?

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.type == .keyDown else { return super.performKeyEquivalent(with: event) }

        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let key = event.charactersIgnoringModifiers?.lowercased()

        if flags.contains(.command), key == "w" {
            performClose(nil)
            return true
        }

        if flags.contains(.command), key == "f" {
            onCommandF?()
            return true
        }

        if event.keyCode == 53 {
            onEscape?()
            return true
        }

        return super.performKeyEquivalent(with: event)
    }
}

// MARK: - VisualEffect

struct VisualEffect: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.blendingMode = .behindWindow
        view.state = .active
        view.material = .popover
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {}
}

// MARK: - EventMonitor

final class EventMonitor {
    private var localMonitor: Any?
    private var globalMonitor: Any?
    private let mask: NSEvent.EventTypeMask
    private let handler: (NSEvent?) -> Void

    init(mask: NSEvent.EventTypeMask, handler: @escaping (NSEvent?) -> Void) {
        self.mask = mask
        self.handler = handler
    }

    deinit { stop() }

    func start() {
        if localMonitor == nil {
            localMonitor = NSEvent.addLocalMonitorForEvents(matching: mask) { [weak self] event in
                self?.handler(event)
                return event
            }
        }
        if globalMonitor == nil {
            globalMonitor = NSEvent.addGlobalMonitorForEvents(matching: mask, handler: handler)
        }
    }

    func stop() {
        if let localMonitor {
            NSEvent.removeMonitor(localMonitor)
            self.localMonitor = nil
        }
        if let globalMonitor {
            NSEvent.removeMonitor(globalMonitor)
            self.globalMonitor = nil
        }
    }
}

private enum StatusBadge {
    case count(Int, NSColor)
    case symbol(String, NSColor)
    case dot(NSColor)
}

private func drawBadge(_ badge: StatusBadge, in bounds: NSRect) {
    switch badge {
    case let .count(value, color):
        let text = "\(value)"
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 7, weight: .bold),
            .foregroundColor: NSColor.white
        ]
        let textSize = text.size(withAttributes: attributes)
        let width = max(10, textSize.width + 5)
        let badgeRect = NSRect(x: bounds.maxX - width - 1, y: bounds.maxY - 10, width: width, height: 10)
        NSBezierPath(roundedRect: badgeRect, xRadius: 5, yRadius: 5).addClip()
        color.setFill()
        badgeRect.fill()
        text.draw(
            at: NSPoint(
                x: badgeRect.minX + (badgeRect.width - textSize.width) / 2,
                y: badgeRect.minY + (badgeRect.height - textSize.height) / 2 - 0.2
            ),
            withAttributes: attributes
        )
    case let .symbol(symbol, color):
        let badgeRect = NSRect(x: bounds.maxX - 10, y: bounds.maxY - 10, width: 9, height: 9)
        color.setFill()
        NSBezierPath(ovalIn: badgeRect).fill()
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 7, weight: .bold),
            .foregroundColor: NSColor.white
        ]
        let textSize = symbol.size(withAttributes: attributes)
        symbol.draw(
            at: NSPoint(
                x: badgeRect.minX + (badgeRect.width - textSize.width) / 2,
                y: badgeRect.minY + (badgeRect.height - textSize.height) / 2 - 0.4
            ),
            withAttributes: attributes
        )
    case let .dot(color):
        let dotRect = NSRect(x: bounds.maxX - 7.5, y: bounds.maxY - 7.5, width: 5.5, height: 5.5)
        color.setFill()
        NSBezierPath(ovalIn: dotRect).fill()
    }
}
