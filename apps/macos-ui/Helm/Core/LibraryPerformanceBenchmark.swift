import Foundation

struct LibraryPerformanceSampleSummary: Equatable {
    let medianMilliseconds: Double
    let p95Milliseconds: Double
    let worstMilliseconds: Double

    init?(samples: [Double]) {
        guard !samples.isEmpty else { return nil }
        let sorted = samples.sorted()
        let middle = sorted.count / 2
        medianMilliseconds = sorted.count.isMultiple(of: 2)
            ? (sorted[middle - 1] + sorted[middle]) / 2
            : sorted[middle]
        let p95Index = max(0, Int(ceil(Double(sorted.count) * 0.95)) - 1)
        p95Milliseconds = sorted[p95Index]
        worstMilliseconds = sorted[sorted.count - 1]
    }
}

enum LibraryPerformanceBenchmarkConfiguration {
    static let iterationsEnvironmentKey = "HELM_LIBRARY_BENCHMARK_ITERATIONS"
    static let minimumIterations = 30
    static let maximumIterations = 100

    static func iterations(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Int? {
        #if DEBUG
        guard let rawValue = environment[iterationsEnvironmentKey],
              let value = Int(rawValue.trimmingCharacters(in: .whitespacesAndNewlines)),
              (minimumIterations...maximumIterations).contains(value) else {
            return nil
        }
        return value
        #else
        return nil
        #endif
    }
}

struct LibraryPackageIndexMemberFacts: Equatable {
    let managerID: String
    let status: String
    let isPinned: Bool
    let normalizedIdentityKey: String
    let managerSearchText: String
    let summarySearchText: String
}

enum LibraryPackageIndexMatchPolicy {
    static func matches(
        _ member: LibraryPackageIndexMemberFacts,
        queryToken: String,
        normalizedQueryToken: String,
        managerID: String?,
        statusFilter: String?,
        pinnedOnly: Bool,
        managerParticipatesInSearch: (String) -> Bool
    ) -> Bool {
        if !queryToken.isEmpty {
            guard managerParticipatesInSearch(member.managerID) else { return false }
            let matchesQuery = member.normalizedIdentityKey.contains(queryToken)
                || (
                    !normalizedQueryToken.isEmpty
                        && normalizedQueryToken != queryToken
                        && member.normalizedIdentityKey.contains(normalizedQueryToken)
                )
                || member.managerSearchText.contains(queryToken)
                || member.summarySearchText.contains(queryToken)
            guard matchesQuery else { return false }
        }
        if let managerID, member.managerID != managerID {
            return false
        }
        if pinnedOnly, !member.isPinned {
            return false
        }
        if let statusFilter {
            if statusFilter == "upgradable", member.isPinned {
                return false
            }
            if member.status != statusFilter,
               !(statusFilter == "installed" && member.status == "upgradable") {
                return false
            }
        }
        return true
    }
}
