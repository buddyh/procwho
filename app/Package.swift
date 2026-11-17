// swift-tools-version: 6.1
import PackageDescription

let package = Package(
    name: "ProcwhoBar",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "ProcwhoBar", targets: ["ProcwhoBar"]),
    ],
    targets: [
        // Models, decoding, the CLI runner and formatters. No UI, so it is unit-testable.
        .target(name: "ProcwhoKit"),
        // SwiftUI menu bar app.
        .executableTarget(name: "ProcwhoBar", dependencies: ["ProcwhoKit"]),
        .testTarget(name: "ProcwhoBarTests", dependencies: ["ProcwhoBar"], path: "BarTests"),
        .testTarget(
            name: "ProcwhoKitTests",
            dependencies: ["ProcwhoKit"],
            path: "Tests",
            sources: ["ProcwhoKitTests"],
            resources: [.copy("Fixtures")]
        ),
    ],
    swiftLanguageModes: [.v6]
)
