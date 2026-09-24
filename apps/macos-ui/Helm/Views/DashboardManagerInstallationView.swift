import SwiftUI

struct DashboardAddManagerCard: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var localization = LocalizationManager.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var showingChooser = false
    @State private var requestedManagerID: String?

    var body: some View {
        Button {
            requestedManagerID = nil
            showingChooser = true
        } label: {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .top) {
                    Text("app.managers.add.title".localized)
                        .font(.headline)
                        .lineLimit(2, reservesSpace: true)
                    Spacer()
                    Image(systemName: "plus.circle")
                        .font(.title2)
                }
                Text("app.managers.add.card_detail".localized)
                    .font(.caption)
                    .foregroundColor(HelmTheme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(HelmTheme.selectionFill, in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12)
                .strokeBorder(HelmTheme.selectionStroke, style: StrokeStyle(lineWidth: 1, dash: [5, 4])))
            .contentShape(RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("app.managers.add.title".localized)
        .accessibilityHint("app.managers.add.card_detail".localized)
        .sheet(isPresented: $showingChooser, onDismiss: openRequestedReview) {
            DashboardManagerChooser { managerID in
                guard core.managerInstallationCatalog.canReview(managerID) else { return }
                requestedManagerID = managerID
                showingChooser = false
            }
        }
    }

    private func openRequestedReview() {
        defer { requestedManagerID = nil }
        guard let managerID = requestedManagerID,
              core.managerInstallationCatalog.canReview(managerID) else { return }
        context.openManagerInstallationReview(for: managerID)
    }
}

private struct DashboardManagerChooser: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var localization = LocalizationManager.shared
    @Environment(\.dismiss) private var dismiss
    @State private var selectedID: String?
    let onReview: (String) -> Void

    private var catalog: ManagerInstallationCatalog { core.managerInstallationCatalog }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("app.managers.add.title".localized)
                .font(.title2.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
            Text("app.managers.add.intro".localized)
                .foregroundColor(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            if let key = availabilityMessage {
                Label(key.localized, systemImage: "info.circle")
                    .foregroundColor(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    if catalog.candidates.isEmpty && catalog.availability == .ready {
                        Text("app.managers.add.empty".localized)
                            .foregroundColor(.secondary)
                            .padding(.vertical, 16)
                    }
                    ForEach(catalog.candidates) { candidate in
                        candidateRow(candidate)
                    }
                }
                .padding(2)
            }

            HStack {
                Button(L10n.Common.cancel.localized) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("app.managers.add.review".localized) {
                    guard let selectedID, catalog.canReview(selectedID) else { return }
                    onReview(selectedID)
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
                .disabled(!catalog.canReview(selectedID))
            }
        }
        .padding(24)
        .frame(width: 540, height: 540)
    }

    private func candidateRow(_ candidate: ManagerInstallationCatalog.Candidate) -> some View {
        Button {
            selectedID = candidate.id
        } label: {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: selectedID == candidate.id ? "checkmark.circle.fill" : "circle")
                    .foregroundColor(HelmTheme.actionPrimaryDefault)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 6) {
                    Text(localizedManagerDisplayName(candidate.id)).font(.headline)
                    if let manager = ManagerInfo.find(byId: candidate.id) {
                        Text(manager.authority.key.localized)
                            .font(.caption).foregroundColor(.secondary)
                    }
                    ForEach(candidate.methods) { choice in
                        Text(methodDescription(choice))
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .helmCardSurface(cornerRadius: 10)
            .overlay(RoundedRectangle(cornerRadius: 10)
                .strokeBorder(selectedID == candidate.id ? HelmTheme.selectionStroke : .clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!catalog.canReview(candidate.id))
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selectedID == candidate.id ? .isSelected : [])
    }

    private var availabilityMessage: String? {
        switch catalog.availability {
        case .ready: return nil
        case .loading: return "app.managers.add.loading"
        case .disconnected: return "app.managers.add.disconnected"
        case .offline: return "app.managers.add.offline"
        case .research: return "app.managers.add.research"
        }
    }

    private func methodDescription(_ choice: ManagerInstallationCatalog.MethodChoice) -> String {
        let method = ManagerDistributionMethod(rawValue: choice.id)?.localizedName ?? choice.id
        let detail: String
        switch choice.block {
        case .policy: detail = "app.managers.add.policy".localized
        case .busy: detail = "app.managers.add.busy".localized
        case let .dependency(id):
            detail = "app.managers.add.dependency".localized(with: ["manager": localizedManagerDisplayName(id)])
        case nil:
            if let id = choice.method.dependencyID {
                detail = "app.managers.add.uses".localized(with: ["manager": localizedManagerDisplayName(id)])
            } else {
                detail = "app.managers.add.direct".localized
            }
        }
        return "\(method): \(detail)"
    }
}
