import AppKit
import SwiftUI

@main
@MainActor
struct HelmDesignLab {
  static func main() throws {
    let outputDirectory = try outputDirectory(from: CommandLine.arguments)
    try FileManager.default.createDirectory(at: outputDirectory, withIntermediateDirectories: true)

    for artifact in ProposalArtifact.allCases {
      for scheme in artifact.schemes {
        let schemeName = scheme == .light ? "light" : "dark"
        let rendered = artifactView(artifact, scheme: scheme)
        let renderer = ImageRenderer(content: rendered.view)
        renderer.scale = 1
        renderer.proposedSize = ProposedViewSize(width: rendered.width, height: rendered.height)

        guard let image = renderer.nsImage,
          let tiff = image.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff),
          let png = bitmap.representation(using: .png, properties: [:])
        else {
          throw RenderError.failed(artifact.rawValue, schemeName)
        }

        let destination =
          outputDirectory
          .appendingPathComponent("\(artifact.rawValue)-\(schemeName).png")
        try png.write(to: destination, options: .atomic)
        print("Rendered \(destination.path)")
      }
    }
  }

  private static func artifactView(
    _ artifact: ProposalArtifact,
    scheme: ColorScheme
  ) -> (view: AnyView, width: CGFloat, height: CGFloat) {
    switch artifact {
    case .dashboardOverview:
      return dashboard(.dashboard, scheme: scheme)
    case .dashboardPlan:
      return dashboard(.plan, scheme: scheme)
    case .dashboardLibrary:
      return dashboard(.library, scheme: scheme)
    case .popoverAttention:
      return popover(.attention, scheme: scheme)
    case .popoverActive:
      return popover(.active, scheme: scheme)
    case .briefingDashboard:
      return fixedView(BriefingDashboardProposal(), width: 1348, height: 868, scheme: scheme)
    case .briefingPopover:
      return fixedView(BriefingPopoverProposal(), width: 456, height: 430, scheme: scheme)
    case .atlasDashboard:
      return fixedView(AtlasDashboardProposal(), width: 1348, height: 868, scheme: scheme)
    case .atlasPopover:
      return fixedView(AtlasPopoverProposal(), width: 476, height: 410, scheme: scheme)
    }
  }

  private static func fixedView<Content: View>(
    _ content: Content,
    width: CGFloat,
    height: CGFloat,
    scheme: ColorScheme
  ) -> (view: AnyView, width: CGFloat, height: CGFloat) {
    let view = content.environment(\.colorScheme, scheme)
    return (AnyView(view), width, height)
  }

  private static func dashboard(
    _ destination: DashboardDestination,
    scheme: ColorScheme
  ) -> (view: AnyView, width: CGFloat, height: CGFloat) {
    let view = DashboardProposal(destination: destination)
      .environment(\.colorScheme, scheme)
    return (AnyView(view), 1348, 868)
  }

  private static func popover(
    _ mode: PopoverProposal.Mode,
    scheme: ColorScheme
  ) -> (view: AnyView, width: CGFloat, height: CGFloat) {
    let view = PopoverProposal(mode: mode)
      .environment(\.colorScheme, scheme)
    return (AnyView(view), 456, mode == .active ? 525 : 515)
  }

  private static func outputDirectory(from arguments: [String]) throws -> URL {
    guard let flagIndex = arguments.firstIndex(of: "--output"),
      arguments.indices.contains(flagIndex + 1)
    else {
      throw RenderError.usage
    }

    let path = arguments[flagIndex + 1]
    return URL(
      fileURLWithPath: path,
      relativeTo: URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    ).standardizedFileURL
  }
}

private enum RenderError: LocalizedError {
  case usage
  case failed(String, String)

  var errorDescription: String? {
    switch self {
    case .usage:
      "Usage: HelmDesignLab --output <directory>"
    case .failed(let direction, let scheme):
      "Could not render \(direction) in \(scheme) appearance"
    }
  }
}
