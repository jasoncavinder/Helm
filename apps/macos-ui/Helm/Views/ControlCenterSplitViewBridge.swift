import SwiftUI

enum NativeSidebarVisibilityPolicy {
    static func splitViewVisibility(isSidebarVisible: Bool) -> NavigationSplitViewVisibility {
        isSidebarVisible ? .all : .detailOnly
    }

    static func isSidebarVisible(for visibility: NavigationSplitViewVisibility) -> Bool {
        visibility != .detailOnly
    }
}
