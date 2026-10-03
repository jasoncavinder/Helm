// swift-tools-version: 5.9
import PackageDescription
import Foundation

// A content-addressed archive path makes Rust changes invalidate SwiftPM's link
// commands. This is a build input, never an accepted runtime/IPC override.
// Keep metadata/resolve commands usable without building Rust. A real build
// without the prepared archive fails at the explicit missing input, never links
// a similarly named library found through a machine-wide search path.
let policyLibraryDirectory = ProcessInfo.processInfo.environment["HELM_EXTERNAL_POLICY_LIB_DIR"]
    ?? URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        .appendingPathComponent(".build/BUILD_RUST_BRIDGE_FIRST").path

let package = Package(
    name: "HelmExternalUpdateObservation",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "HelmExternalUpdateObservation", targets: ["HelmExternalUpdateObservation"]),
        .executable(name: "helm-external-observe", targets: ["ObservationProbe"]),
        .executable(name: "helm-external-policy-probe", targets: ["PolicyProbe"]),
        .executable(name: "HelmSparkleExternalUpdater", targets: ["ExternalUpdaterHost"]),
        .executable(name: "helm-external-bootstrap-probe", targets: ["BootstrapProbe"])
    ],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", exact: "2.9.5")
    ],
    targets: [
        .target(name: "CExternalUpdatePolicy", linkerSettings: [
            .unsafeFlags([policyLibraryDirectory + "/libhelm_external_update_bridge.a"])
        ]),
        .target(name: "HelmExternalUpdateObservation", dependencies: ["CExternalUpdatePolicy"]),
        .executableTarget(name: "ObservationProbe", dependencies: ["HelmExternalUpdateObservation"]),
        .executableTarget(name: "PolicyProbe", dependencies: ["HelmExternalUpdateObservation"]),
        .executableTarget(name: "BootstrapProbe", dependencies: ["HelmExternalUpdateObservation"]),
        .executableTarget(
            name: "ExternalUpdaterHost",
            dependencies: ["HelmExternalUpdateObservation", .product(name: "Sparkle", package: "Sparkle")],
            linkerSettings: [.unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"])]
        ),
        .testTarget(name: "HelmExternalUpdateObservationTests", dependencies: ["HelmExternalUpdateObservation", "CExternalUpdatePolicy"])
    ]
)
