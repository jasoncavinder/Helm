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

    func begin(managerID: String, authorityKey: String, installedOrder: [String], rowHeight: CGFloat) -> UUID? {
        guard session == nil,
              let next = ManagerPriorityReorderSession(
                managerID: managerID, authorityKey: authorityKey, installedOrder: installedOrder
              ) else { return nil }
        self.rowHeight = rowHeight.isFinite ? max(60, rowHeight) : 100
        session = next
        return next.id
    }

    @discardableResult
    func propose(targetID: String, after: Bool, authorityKey: String) -> Bool {
        guard var next = session, next.propose(targetID: targetID, after: after, authorityKey: authorityKey) else { return false }
        if next != session { session = next }
        return true
    }

    func cancel(id: UUID? = nil) {
        guard id == nil || session?.id == id else { return }
        session = nil
        nativeSource = nil
    }
}
