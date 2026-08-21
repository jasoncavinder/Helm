import Combine

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
