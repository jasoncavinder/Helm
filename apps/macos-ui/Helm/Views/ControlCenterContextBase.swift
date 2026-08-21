import Combine
import SwiftUI

private struct ControlCenterLocaleRevisionKey: EnvironmentKey {
    static let defaultValue = 0
}

extension EnvironmentValues {
    var controlCenterLocaleRevision: Int {
        get { self[ControlCenterLocaleRevisionKey.self] }
        set { self[ControlCenterLocaleRevisionKey.self] = newValue }
    }
}

struct ControlCenterLocaleRefreshHost<Content: View>: View {
    let revision: Int
    private let content: Content

    init(revision: Int, @ViewBuilder content: () -> Content) {
        self.revision = revision
        self.content = content()
    }

    var body: some View {
        content.environment(\.controlCenterLocaleRevision, revision)
    }
}

class ControlCenterContextBase: ObservableObject {
    @Published var selectedTaskId: String?
    @Published private(set) var localeRevision = 0
    private var localizationObservation: AnyCancellable?

    init(localizationChanges: AnyPublisher<Void, Never>) {
        localizationObservation = localizationChanges.sink { [weak self] _ in
            self?.localeRevision += 1
        }
    }
}
