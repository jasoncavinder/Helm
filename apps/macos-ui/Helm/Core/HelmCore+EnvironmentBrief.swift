import Foundation

extension HelmCore {
    func makeEnvironmentBriefInput(
        intendedManagers: [ManagerInfo]
    ) -> EnvironmentBriefProjectionInput {
        let freshness: EnvironmentBriefFreshness = isConnected ? .current : .cached
        let observations = managerStatuses.values
            .filter(\.isImplemented)
            .map { status in
                EnvironmentBriefManagerObservation(
                    manager: status.managerId,
                    detected: status.detected,
                    eligibility: environmentBriefEligibility(for: status),
                    managementState: environmentBriefManagementState(for: status),
                    activeInstallationMethod: status.selectedInstallMethod,
                    provenance: status.detected
                        ? EnvironmentBriefProvenance(
                            rawValue: status.activeProvenance ?? "unknown"
                        ) ?? .unknown
                        : nil,
                    freshness: freshness
                )
            }

        var latestDetectionTaskByManager: [String: CoreTaskRecord] = [:]
        for task in latestCoreTasksSnapshot where task.taskType.lowercased() == "detection" {
            if let latestTaskID = latestDetectionTaskByManager[task.manager]?.id,
               task.id <= latestTaskID {
                continue
            } else {
                latestDetectionTaskByManager[task.manager] = task
            }
        }

        let failedManagerIDs = latestDetectionTaskByManager.values.compactMap { task in
            let status = task.status.lowercased()
            return status == "failed" || status == "interrupted" ? task.manager : nil
        }
        let cancelledManagerIDs = latestDetectionTaskByManager.values.compactMap { task in
            task.status.lowercased() == "cancelled" ? task.manager : nil
        }

        return EnvironmentBriefProjectionInput(
            system: .current(
                distributionChannel: AppUpdateCoordinator.shared.distributionChannel.rawValue,
                updateAuthority: AppUpdateCoordinator.shared.updateAuthority.rawValue
            ),
            intendedManagerIDs: intendedManagers.map(\.id),
            observations: observations,
            failedManagerIDs: failedManagerIDs,
            cancelledManagerIDs: cancelledManagerIDs,
            deferredManagerIDs: [],
            observationClass: .localOnly
        )
    }

    private func environmentBriefEligibility(
        for status: ManagerStatus
    ) -> EnvironmentBriefEligibility {
        guard let isEligible = status.isEligible else { return .unknown }
        return isEligible ? .eligible : .ineligible
    }

    private func environmentBriefManagementState(
        for status: ManagerStatus
    ) -> EnvironmentBriefManagementState {
        guard status.detected else { return .notInstalled }
        if status.isEligible == false {
            return .detectedUnmanageable
        }
        if status.multiInstanceState == "attention_needed" {
            return .multipleInstancesAttention
        }
        if status.packageStateIssues?.contains(where: { issue in
            issue.issueCode == "post_install_setup_required"
        }) == true {
            return .setupRequired
        }
        return .ready
    }
}
