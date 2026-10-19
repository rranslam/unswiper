// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "Unswiper",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "Unswiper", targets: ["Unswiper"])
    ],
    targets: [
        .executableTarget(
            name: "Unswiper",
            path: "Sources/Unswiper",
            resources: [.copy("Resources/Help"), .copy("Resources/Takeout")]
        ),
        .testTarget(
            name: "UnswiperTests",
            dependencies: ["Unswiper"],
            path: "Tests/UnswiperTests"
        )
    ]
)
