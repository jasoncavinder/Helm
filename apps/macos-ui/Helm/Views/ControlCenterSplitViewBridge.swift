import AppKit
import SwiftUI

struct ControlCenterSplitViewBridge<Content: View, Inspector: View>: NSViewControllerRepresentable {
    let isInspectorPresented: Bool
    let contentMinimumThickness: CGFloat
    private let content: Content
    private let inspector: Inspector

    init(
        isInspectorPresented: Bool,
        contentMinimumThickness: CGFloat = 400,
        @ViewBuilder content: () -> Content,
        @ViewBuilder inspector: () -> Inspector
    ) {
        self.isInspectorPresented = isInspectorPresented
        self.contentMinimumThickness = contentMinimumThickness
        self.content = content()
        self.inspector = inspector()
    }

    func makeNSViewController(context: Context) -> ControlCenterSplitViewController {
        ControlCenterSplitViewController(
            content: AnyView(content),
            contentMinimumThickness: contentMinimumThickness,
            inspector: { AnyView(inspector) }
        )
    }

    func updateNSViewController(
        _ splitViewController: ControlCenterSplitViewController,
        context: Context
    ) {
        splitViewController.setInspectorPresented(isInspectorPresented)
    }

    static func dismantleNSViewController(
        _ splitViewController: ControlCenterSplitViewController,
        coordinator: Void
    ) {
        splitViewController.setInspectorPresented(false)
    }
}

final class ControlCenterSplitViewController: NSSplitViewController {
    private let inspectorProvider: () -> AnyView
    private let contentHostingController: NSHostingController<AnyView>
    private let inspectorHostingController: NSHostingController<AnyView>
    private let contentSplitViewItem: NSSplitViewItem
    private let inspectorSplitViewItem: NSSplitViewItem

    private(set) var isInspectorContentMounted = false

    var isInspectorCollapsed: Bool {
        inspectorSplitViewItem.isCollapsed
    }

    init(
        content: AnyView,
        contentMinimumThickness: CGFloat = 400,
        inspector: @escaping () -> AnyView
    ) {
        inspectorProvider = inspector
        contentHostingController = NSHostingController(rootView: content)
        inspectorHostingController = NSHostingController(rootView: AnyView(EmptyView()))
        contentSplitViewItem = NSSplitViewItem(viewController: contentHostingController)
        inspectorSplitViewItem = NSSplitViewItem(viewController: inspectorHostingController)
        super.init(nibName: nil, bundle: nil)
        contentSplitViewItem.minimumThickness = contentMinimumThickness
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        splitView.isVertical = true
        splitView.dividerStyle = .thin

        inspectorSplitViewItem.minimumThickness = 220
        inspectorSplitViewItem.maximumThickness = 320
        inspectorSplitViewItem.preferredThicknessFraction = 0.25
        inspectorSplitViewItem.canCollapse = true

        addSplitViewItem(contentSplitViewItem)
        addSplitViewItem(inspectorSplitViewItem)
        inspectorSplitViewItem.isCollapsed = true
    }

    func setInspectorPresented(_ isPresented: Bool) {
        _ = view

        if isPresented {
            if !isInspectorContentMounted {
                inspectorHostingController.rootView = inspectorProvider()
                isInspectorContentMounted = true
            }
            inspectorSplitViewItem.isCollapsed = false
        } else {
            inspectorSplitViewItem.isCollapsed = true
            if isInspectorContentMounted {
                inspectorHostingController.rootView = AnyView(EmptyView())
                isInspectorContentMounted = false
            }
        }
    }
}
