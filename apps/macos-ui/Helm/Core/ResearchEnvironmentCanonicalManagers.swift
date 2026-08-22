enum ResearchManagerHealthState: Equatable {
    case healthy
    case needsReview
    case error
    case unavailable
    case notInstalled
}

enum ResearchManagerHealthPolicy {
    static func health(
        for manager: ResearchManagerRecord,
        decisionState: ResearchManagerDecisionState,
        isDecisionTarget: Bool
    ) -> ResearchManagerHealthState {
        if isDecisionTarget, decisionState == .acknowledged {
            return .healthy
        }
        switch manager.sourceState {
        case "failed":
            return .error
        case "deferred":
            return .unavailable
        case "needs_review":
            return .needsReview
        default:
            return manager.detected ? .healthy : .notInstalled
        }
    }
}

extension WholeWorkflowResearchTaskFiveContract {
    static let canonicalManagers = [
        ResearchManagerRecord(
            id: "mise",
            authority: "authoritative",
            detected: true,
            enabled: true,
            freshness: "current",
            sourceState: "ready",
            installInstances: [
                ResearchInstallInstanceRecord(
                    id: "mise-user",
                    displayPath: "<home>/.local/bin/mise",
                    provenance: "mise",
                    active: true,
                    policyState: "manageable"
                ),
            ],
            findingCode: nil
        ),
        ResearchManagerRecord(
            id: managerID,
            authority: "authoritative",
            detected: true,
            enabled: true,
            freshness: "current",
            sourceState: "needs_review",
            installInstances: [
                ResearchInstallInstanceRecord(
                    id: systemInstanceID,
                    displayPath: "/usr/bin/rustup",
                    provenance: "system",
                    active: false,
                    policyState: "policy_blocked"
                ),
                ResearchInstallInstanceRecord(
                    id: userInstanceID,
                    displayPath: "<home>/.cargo/bin/rustup",
                    provenance: "rustup_init",
                    active: true,
                    policyState: "manageable"
                ),
            ],
            findingCode: "multiple_installs_detected"
        ),
        ResearchManagerRecord(
            id: "homebrew_formula",
            authority: "guarded",
            detected: true,
            enabled: true,
            freshness: "current",
            sourceState: "ready",
            installInstances: [
                ResearchInstallInstanceRecord(
                    id: "homebrew-formula-arm64",
                    displayPath: "/opt/homebrew/bin/brew",
                    provenance: "homebrew",
                    active: true,
                    policyState: "manageable"
                ),
            ],
            findingCode: nil
        ),
        ResearchManagerRecord(
            id: "homebrew_cask",
            authority: "standard",
            detected: true,
            enabled: true,
            freshness: "cached",
            sourceState: "ready",
            installInstances: [
                ResearchInstallInstanceRecord(
                    id: "homebrew-cask-arm64",
                    displayPath: "/opt/homebrew/bin/brew",
                    provenance: "homebrew",
                    active: true,
                    policyState: "manageable"
                ),
            ],
            findingCode: nil
        ),
        ResearchManagerRecord(
            id: "npm",
            authority: "standard",
            detected: true,
            enabled: true,
            freshness: "current",
            sourceState: "ready",
            installInstances: [
                ResearchInstallInstanceRecord(
                    id: "npm-mise-node",
                    displayPath: "<home>/.local/share/mise/shims/npm",
                    provenance: "mise",
                    active: true,
                    policyState: "manageable"
                ),
            ],
            findingCode: nil
        ),
        ResearchManagerRecord(
            id: "cargo",
            authority: "standard",
            detected: true,
            enabled: true,
            freshness: "current",
            sourceState: "ready",
            installInstances: [
                ResearchInstallInstanceRecord(
                    id: "cargo-user",
                    displayPath: "<home>/.cargo/bin/cargo",
                    provenance: "rustup_init",
                    active: true,
                    policyState: "manageable"
                ),
            ],
            findingCode: nil
        ),
        ResearchManagerRecord(
            id: "pnpm",
            authority: "standard",
            detected: true,
            enabled: true,
            freshness: "current",
            sourceState: "ready",
            installInstances: [
                ResearchInstallInstanceRecord(
                    id: "pnpm-user",
                    displayPath: "<home>/.local/share/mise/shims/pnpm",
                    provenance: "mise",
                    active: true,
                    policyState: "manageable"
                ),
            ],
            findingCode: nil
        ),
        ResearchManagerRecord(
            id: "mas",
            authority: "standard",
            detected: true,
            enabled: true,
            freshness: "current",
            sourceState: "ready",
            installInstances: [
                ResearchInstallInstanceRecord(
                    id: "mas-homebrew",
                    displayPath: "/opt/homebrew/bin/mas",
                    provenance: "homebrew",
                    active: true,
                    policyState: "needs_confirmation"
                ),
            ],
            findingCode: nil
        ),
        ResearchManagerRecord(
            id: "softwareupdate",
            authority: "guarded",
            detected: true,
            enabled: true,
            freshness: "unknown",
            sourceState: "deferred",
            installInstances: [
                ResearchInstallInstanceRecord(
                    id: "softwareupdate-system",
                    displayPath: "/usr/sbin/softwareupdate",
                    provenance: "system",
                    active: true,
                    policyState: "needs_confirmation"
                ),
            ],
            findingCode: nil
        ),
        ResearchManagerRecord(
            id: "macports",
            authority: "guarded",
            detected: false,
            enabled: true,
            freshness: "unknown",
            sourceState: "failed",
            installInstances: [],
            findingCode: "source_refresh_failed"
        ),
    ]

    static let orderedManagerIDs = canonicalManagers.map(\.id)
}
