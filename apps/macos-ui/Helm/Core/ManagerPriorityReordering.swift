import Combine
import Foundation

/// A presentation-only preview. Persisted priority is untouched until a valid drop.
struct ManagerPriorityReorderSession: Equatable {
    let id: UUID
    let authorityKey: String
    let managerID: String
    let originalOrder: [String]
    private(set) var proposedOrder: [String]

    init?(managerID: String, authorityKey: String, installedOrder: [String], id: UUID = UUID()) {
        guard installedOrder.contains(managerID), Set(installedOrder).count == installedOrder.count else { return nil }
        self.id = id
        self.authorityKey = authorityKey
        self.managerID = managerID
        originalOrder = installedOrder
        proposedOrder = installedOrder
    }

    mutating func propose(targetID: String, after: Bool, authorityKey: String) -> Bool {
        guard authorityKey == self.authorityKey, targetID != managerID,
              originalOrder.contains(targetID) else { return false }
        var order = originalOrder.filter { $0 != managerID }
        guard let target = order.firstIndex(of: targetID) else { return false }
        order.insert(managerID, at: target + (after ? 1 : 0))
        proposedOrder = order
        return true
    }

    func validatedOrder(currentInstalledOrder: [String], authorityKey: String) -> [String]? {
        guard authorityKey == self.authorityKey, currentInstalledOrder == originalOrder,
              Set(proposedOrder) == Set(originalOrder), proposedOrder.count == originalOrder.count,
              proposedOrder.filter({ $0 != managerID }) == originalOrder.filter({ $0 != managerID }) else { return nil }
        return proposedOrder
    }
}

final class ManagerPriorityDragState: ObservableObject {
    @Published private(set) var session: ManagerPriorityReorderSession?
    private(set) var rowHeight: CGFloat = 100
    // Keep the native source alive even while its SwiftUI row becomes a placeholder.
    var nativeSource: AnyObject?
    private var motionExtreme: CGFloat?
    private var movingDown: Bool?

    func begin(managerID: String, authorityKey: String, installedOrder: [String], rowHeight: CGFloat) -> UUID? {
        guard session == nil,
              let next = ManagerPriorityReorderSession(
                managerID: managerID, authorityKey: authorityKey, installedOrder: installedOrder
              ) else { return nil }
        self.rowHeight = rowHeight.isFinite ? max(60, rowHeight) : 100
        motionExtreme = nil
        movingDown = nil
        session = next
        return next.id
    }

    @discardableResult
    func propose(targetID: String, after: Bool, authorityKey: String) -> Bool {
        guard var next = session, next.propose(targetID: targetID, after: after, authorityKey: authorityKey) else { return false }
        if next != session { session = next }
        return true
    }

    /// All geometry uses the scroll document's top-to-bottom coordinates.
    @discardableResult
    func proposeOverlap(dragFrame: CGRect, pointerY: CGFloat, targetFrames: [String: CGRect],
                        authorityKey: String, token: UUID) -> Bool {
        guard let session, session.id == token, session.authorityKey == authorityKey,
              pointerY.isFinite, Self.validFrame(dragFrame),
              let sourceIndex = session.proposedOrder.firstIndex(of: session.managerID) else { return false }
        guard let extreme = motionExtreme else {
            motionExtreme = pointerY
            return false
        }
        let delta = pointerY - extreme
        guard delta != 0 else { return false }
        let down = delta > 0
        // Reflow is not pointer motion. Ignore tiny reversals after a card moves under the drag.
        guard movingDown == nil || movingDown == down || abs(delta) >= 4 else { return false }
        movingDown = down
        motionExtreme = pointerY

        let candidates = session.proposedOrder.enumerated().filter { index, id in
            guard down ? index > sourceIndex : index < sourceIndex,
                  let frame = targetFrames[id], Self.validFrame(frame) else { return false }
            return dragFrame.maxX >= frame.minX && dragFrame.minX <= frame.maxX
                && dragFrame.maxY >= frame.minY && dragFrame.minY <= frame.maxY
        }
        guard let target = down ? candidates.last : candidates.first else { return false }
        return propose(targetID: target.element, after: down, authorityKey: authorityKey)
    }

    private static func validFrame(_ frame: CGRect) -> Bool {
        frame.origin.x.isFinite && frame.origin.y.isFinite
            && frame.width.isFinite && frame.height.isFinite && frame.width > 0 && frame.height > 0
    }

    func cancel(id: UUID? = nil) {
        guard id == nil || session?.id == id else { return }
        session = nil
        nativeSource = nil
        motionExtreme = nil
        movingDown = nil
    }
}
