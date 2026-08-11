enum StatusItemClickKind: Equatable {
    case primary
    case secondary
}

enum StatusItemActivationRoute: Equatable {
    case popover
    case dashboard
}

enum StatusItemActivationPolicy {
    static func route(
        clickKind: StatusItemClickKind,
        isDashboardVisible: Bool
    ) -> StatusItemActivationRoute {
        switch clickKind {
        case .secondary:
            return .popover
        case .primary:
            return isDashboardVisible ? .dashboard : .popover
        }
    }
}
