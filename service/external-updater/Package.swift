// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "HelmExternalUpdateObservation",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "HelmExternalUpdateObservation", targets: ["HelmExternalUpdateObservation"]),
        .executable(name: "helm-external-observe", targets: ["ObservationProbe"]),
        .executable(name: "HelmSparkleExternalUpdater", targets: ["ExternalUpdaterHost"]),
        .executable(name: "helm-external-bootstrap-probe", targets: ["BootstrapProbe"])
    ],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", exact: "2.9.5")
    ],
    targets: [
        .target(name: "HelmExternalUpdateObservation"),
        .executableTarget(name: "ObservationProbe", dependencies: ["HelmExternalUpdateObservation"]),
        .executableTarget(name: "BootstrapProbe", dependencies: ["HelmExternalUpdateObservation"]),
        .executableTarget(
            name: "ExternalUpdaterHost",
            dependencies: ["HelmExternalUpdateObservation", .product(name: "Sparkle", package: "Sparkle")],
            linkerSettings: [.unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"])]
        ),
        .testTarget(name: "HelmExternalUpdateObservationTests", dependencies: ["HelmExternalUpdateObservation"])
    ]
)
