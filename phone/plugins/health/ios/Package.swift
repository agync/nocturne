// swift-tools-version:5.9

import PackageDescription

let package = Package(
  name: "tauri-plugin-nocturne-health",
  platforms: [
    .macOS(.v14),
    .iOS(.v17),
  ],
  products: [
    .library(
      name: "tauri-plugin-nocturne-health",
      type: .static,
      targets: ["tauri-plugin-nocturne-health"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-nocturne-health",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources"),
    .testTarget(
      name: "HealthRowsTests",
      dependencies: ["tauri-plugin-nocturne-health"],
      path: "Tests"),
  ]
)
