import Cocoa
import SwiftUI

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

// MARK: - ControlCenterWindow

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
