// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "AkariKit",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "AkariKit", targets: ["AkariKit"]),
    ],
    targets: [
        // Built by apps/apple/build-ffi.sh, like Sources/AkariFFI/Generated; neither is checked in.
        .binaryTarget(name: "akari_ffiFFI", path: "Frameworks/akari_ffiFFI.xcframework"),
        // The generated bindings stay in their own target: they don't compile with MainActor
        // as the default isolation, which app targets may use.
        .target(
            name: "AkariFFI",
            dependencies: ["akari_ffiFFI"],
            linkerSettings: [
                .linkedFramework("CoreFoundation"),
                .linkedFramework("Security"),
                .linkedLibrary("iconv"),
                .linkedLibrary("sqlite3"),
            ]
        ),
        // Resources/animated.webp: the launch probe's input (AnimatedFormatProbe).
        .target(
            name: "AkariKit", dependencies: ["AkariFFI"], resources: [.copy("Resources")]),
        .testTarget(
            name: "AkariKitTests",
            dependencies: ["AkariKit"],
            resources: [.copy("Fixtures")]
        ),
    ]
)
