import Foundation

/// Presentation of planner-supported methods, never a second installation planner.
struct ManagerInstallationCatalog {
    struct Method: Equatable, Identifiable {
        let id: String
        let policyAllowed: Bool
        let dependencyID: String?
    }

    struct Source {
        let id: String
        let detected: Bool
        let enabled: Bool
        let implemented: Bool
        let eligible: Bool
        let busy: Bool
        let methods: [Method]
    }

    enum Block: Equatable {
        case policy
        case dependency(String)
        case busy
    }

    struct MethodChoice: Identifiable, Equatable {
        let method: Method
        let block: Block?
        var id: String { method.id }
    }

    struct Candidate: Identifiable, Equatable {
        let id: String
        let methods: [MethodChoice]
        var canReview: Bool { methods.contains { $0.block == nil } }
    }

    enum Availability: Equatable {
        case ready, loading, disconnected, offline, research
    }

    let candidates: [Candidate]
    let availability: Availability

    init(sources: [Source], connected: Bool, online: Bool, research: Bool) {
        if research {
            availability = .research
        } else if !connected {
            availability = .disconnected
        } else if !online {
            availability = .offline
        } else if sources.isEmpty {
            availability = .loading
        } else {
            availability = .ready
        }
        let byID = Dictionary(sources.map { ($0.id, $0) }, uniquingKeysWith: { _, latest in latest })
        candidates = sources.filter {
            $0.implemented && !$0.detected && !$0.methods.isEmpty
        }.map { source in
            Candidate(id: source.id, methods: source.methods.map { method in
                let block: Block?
                if !source.eligible || !method.policyAllowed {
                    block = .policy
                } else if source.busy {
                    block = .busy
                } else if let dependencyID = method.dependencyID,
                          !Self.dependencyReady(byID[dependencyID]) {
                    block = .dependency(dependencyID)
                } else {
                    block = nil
                }
                return MethodChoice(method: method, block: block)
            })
        }.sorted { $0.id < $1.id }
    }

    func canReview(_ managerID: String?) -> Bool {
        availability == .ready && candidates.contains { $0.id == managerID && $0.canReview }
    }

    func canReview(_ managerID: String, methodID: String) -> Bool {
        canReview(managerID) && candidates.first(where: { $0.id == managerID })?
            .methods.contains(where: { $0.id == methodID && $0.block == nil }) == true
    }

    private static func dependencyReady(_ source: Source?) -> Bool {
        guard let source else { return false }
        return source.detected && source.enabled && source.implemented && source.eligible && !source.busy
    }
}
