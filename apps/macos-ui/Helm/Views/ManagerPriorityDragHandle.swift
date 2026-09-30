import AppKit
import SwiftUI
import UniformTypeIdentifiers

enum ManagerPriorityDragType {
    static let type = UTType(exportedAs: "app.helm.manager-priority", conformingTo: .data)
    static var identifier: String { type.identifier }
}

/// Native completion covers Escape and drops outside Helm, unlike a row-only DropDelegate.
struct ManagerPriorityDragHandle: NSViewRepresentable {
    let title: String
    let state: ManagerPriorityDragState
    let begin: () -> UUID?

    func makeNSView(context: Context) -> Handle {
        let view = Handle()
        view.setAccessibilityElement(false)
        return view
    }

    func updateNSView(_ view: Handle, context: Context) {
        view.title = title
        view.state = state
        view.begin = begin
    }

    final class Handle: NSView {
        var title = ""
        weak var state: ManagerPriorityDragState?
        var begin: (() -> UUID?)?
        private var mouseDownEvent: NSEvent?

        override func resetCursorRects() { addCursorRect(bounds, cursor: .openHand) }
        override func mouseDown(with event: NSEvent) { mouseDownEvent = event }
        override func mouseUp(with event: NSEvent) { mouseDownEvent = nil }

        override func mouseDragged(with event: NSEvent) {
            guard let down = mouseDownEvent, let state,
                  hypot(event.locationInWindow.x - down.locationInWindow.x,
                        event.locationInWindow.y - down.locationInWindow.y) >= 4,
                  let token = begin?() else { return }
            mouseDownEvent = nil
            let source = Source(state: state, token: token, scrollView: enclosingScrollView)
            state.nativeSource = source
            let item = NSPasteboardItem()
            item.setString(token.uuidString, forType: NSPasteboard.PasteboardType(ManagerPriorityDragType.identifier))
            let draggingItem = NSDraggingItem(pasteboardWriter: item)
            let image = Self.dragImage(title)
            draggingItem.setDraggingFrame(NSRect(origin: .zero, size: image.size), contents: image)
            let session = beginDraggingSession(with: [draggingItem], event: down, source: source)
            session.animatesToStartingPositionsOnCancelOrFail = false
        }

        private static func dragImage(_ title: String) -> NSImage {
            let size = NSSize(width: 260, height: 44)
            return NSImage(size: size, flipped: false) { rect in
                NSColor.controlBackgroundColor.setFill()
                NSBezierPath(roundedRect: rect, xRadius: 10, yRadius: 10).fill()
                let paragraph = NSMutableParagraphStyle()
                paragraph.lineBreakMode = .byTruncatingTail
                (title as NSString).draw(in: rect.insetBy(dx: 14, dy: 12), withAttributes: [
                    .font: NSFont.systemFont(ofSize: 14, weight: .medium),
                    .foregroundColor: NSColor.labelColor, .paragraphStyle: paragraph
                ])
                return true
            }
        }
    }

    final class Source: NSObject, NSDraggingSource {
        weak var state: ManagerPriorityDragState?
        let token: UUID
        weak var scrollView: NSScrollView?
        private var timer: Timer?
        private var screenPoint: NSPoint?

        init(state: ManagerPriorityDragState, token: UUID, scrollView: NSScrollView?) {
            self.state = state
            self.token = token
            self.scrollView = scrollView
        }

        func draggingSession(_ session: NSDraggingSession,
                             sourceOperationMaskFor context: NSDraggingContext) -> NSDragOperation {
            context == .withinApplication ? .move : []
        }

        func draggingSession(_ session: NSDraggingSession, willBeginAt screenPoint: NSPoint) {
            self.screenPoint = screenPoint
            let timer = Timer(timeInterval: 0.08, repeats: true) { [weak self] _ in self?.autoscroll() }
            self.timer = timer
            RunLoop.main.add(timer, forMode: .common)
        }

        func draggingSession(_ session: NSDraggingSession, movedTo screenPoint: NSPoint) {
            self.screenPoint = screenPoint
        }

        func draggingSession(_ session: NSDraggingSession, endedAt screenPoint: NSPoint, operation: NSDragOperation) {
            timer?.invalidate()
            timer = nil
            state?.cancel(id: token)
        }

        private func autoscroll() {
            guard state?.session?.id == token, let scrollView, let window = scrollView.window,
                  let screenPoint, let document = scrollView.documentView else { return }
            let point = window.convertPoint(fromScreen: screenPoint)
            let local = scrollView.convert(point, from: nil)
            // Do not scroll unrelated content when the drag leaves this list horizontally.
            guard local.x >= 0, local.x <= scrollView.bounds.width,
                  local.y >= -24, local.y <= scrollView.bounds.height + 24 else { return }
            guard let event = NSEvent.mouseEvent(with: .leftMouseDragged, location: point,
                                                modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                                windowNumber: window.windowNumber, context: nil,
                                                eventNumber: 0, clickCount: 1, pressure: 1) else { return }
            _ = document.autoscroll(with: event)
        }

        deinit { timer?.invalidate() }
    }
}
