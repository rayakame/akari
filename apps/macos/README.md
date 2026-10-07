# Akari for macOS

The first Akari app: a SwiftUI shell with AppKit for the message list. The Xcode
project is generated with [XcodeGen](https://github.com/yonaskolb/XcodeGen). It
consumes `akari-ffi` as an XCFramework / Swift package and uses `AkariKit` for shared
logic and view models. Tokens are stored in the Keychain through the core's
token-storage callback interface.

Building it requires full Xcode (not just the Command Line Tools) and XcodeGen
(`brew install xcodegen`).

Status: not started.
