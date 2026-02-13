import Cocoa
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var panel: FloatingPanel!
    private var eventMonitor: EventMonitor?

    func applicationDidFinishLaunching(_ notification: Notification) {
        let contentView = StatusBarView()
            .background(VisualEffect().ignoresSafeArea())
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))

        panel = FloatingPanel(
            contentRect: NSRect(x: 0, y: 0, width: 350, height: 600),
            backing: .buffered,
            defer: false
        )
        panel.contentViewController = NSHostingController(rootView: contentView)

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem?.button {
            button.image = NSImage(named: "MenuBarIcon")
            button.image?.isTemplate = true
            button.action = #selector(togglePanel(_:))
            button.target = self
        }

        eventMonitor = EventMonitor(mask: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
            if let panel = self?.panel, panel.isVisible {
                self?.closePanel()
            }
        }
    }

    @objc private func togglePanel(_ sender: AnyObject?) {
        if panel.isVisible {
            closePanel()
        } else {
            showPanel()
        }
    }

    private func showPanel() {
        guard let button = statusItem?.button else { return }

        let buttonRect = button.window?.convertToScreen(button.frame) ?? .zero
        let screen = NSScreen.main ?? NSScreen.screens.first

        // Determine max height: from menu bar down to 40pt above screen bottom
        let maxHeight: CGFloat
        if let screen = screen {
            let visibleBottom = screen.visibleFrame.origin.y
            maxHeight = min(buttonRect.origin.y - visibleBottom - 40, 600)
        } else {
            maxHeight = 600
        }

        if let view = panel.contentViewController?.view {
            let size = view.fittingSize
            if size.height > 0 && size.width > 0 {
                let clampedHeight = min(size.height, max(maxHeight, 300))
                panel.setContentSize(NSSize(width: size.width, height: clampedHeight))
            }
        }

        let panelSize = panel.frame.size
        let x = buttonRect.origin.x + (buttonRect.width / 2) - (panelSize.width / 2)
        let y = buttonRect.origin.y - panelSize.height - 5

        panel.setFrameOrigin(NSPoint(x: x, y: y))
        panel.makeKeyAndOrderFront(nil)
        eventMonitor?.start()
    }

    private func closePanel() {
        panel.orderOut(nil)
        eventMonitor?.stop()
    }
}

// MARK: - FloatingPanel

final class FloatingPanel: NSPanel {
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
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
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
    private var monitor: Any?
    private let mask: NSEvent.EventTypeMask
    private let handler: (NSEvent?) -> Void

    init(mask: NSEvent.EventTypeMask, handler: @escaping (NSEvent?) -> Void) {
        self.mask = mask
        self.handler = handler
    }

    deinit { stop() }

    func start() {
        if monitor == nil {
            monitor = NSEvent.addGlobalMonitorForEvents(matching: mask, handler: handler)
        }
    }

    func stop() {
        if let monitor = monitor {
            NSEvent.removeMonitor(monitor)
            self.monitor = nil
        }
    }
}

// MARK: - StatusBarView

private struct StatusBarView: View {
    @StateObject var core = HelmCore.shared
    @State private var selectedTab: HelmTab = .dashboard
    @State private var showSettings: Bool = false

    var body: some View {
        VStack(spacing: 0) {
            NavigationBarView(
                selectedTab: $selectedTab,
                searchText: $core.searchText,
                showSettings: $showSettings
            )

            Divider()

            if core.isInitialized {
                Group {
                    switch selectedTab {
                    case .dashboard:
                        DashboardView()
                    case .packages:
                        PackageListView(searchText: $core.searchText)
                    }
                }
                .frame(maxHeight: .infinity)
            } else {
                VStack {
                    Spacer()
                    ProgressView("Initializing...")
                        .font(.subheadline)
                    Spacer()
                }
            }

            Divider()

            HStack {
                if core.isRefreshing {
                    ProgressView()
                        .scaleEffect(0.5)
                        .frame(width: 12, height: 12)
                    Text("Refreshing...")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                } else {
                    Text("v\(helmVersion)")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
        }
        .frame(width: 400)
        .onChange(of: core.searchText) { newValue in
            if !newValue.trimmingCharacters(in: .whitespaces).isEmpty && selectedTab != .packages {
                selectedTab = .packages
            }
        }
    }
}
