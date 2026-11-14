import AkariKit
import AppKit
import CoreGraphics
import Foundation
import ImageIO
import Testing
import UniformTypeIdentifiers
import os

@testable import Akari

/// An image source for the app's tests: a script per request, every request and every
/// cancellation that reached it recorded.
final class FakeImageSource: ImageSource, @unchecked Sendable {
    let requests = OSAllocatedUnfairLock<[ImageRequest]>(initialState: [])
    let cancelled = OSAllocatedUnfairLock(initialState: 0)
    private let answer: OSAllocatedUnfairLock<@Sendable (ImageRequest) async throws -> ImageFile>

    init(_ answer: @escaping @Sendable (ImageRequest) async throws -> ImageFile) {
        self.answer = OSAllocatedUnfairLock(initialState: answer)
    }

    /// Every request gets `url`, after `gate` fired when one is given.
    static func serving(_ url: URL, pixels: UInt32, animated: Bool = false, gate: Gate? = nil)
        -> FakeImageSource
    {
        FakeImageSource { _ in
            if let gate {
                await gate.wait()
            }
            return ImageFile(
                path: url.path, width: pixels, height: pixels, animated: animated, fromDisk: false)
        }
    }

    /// Stills from `still`, animations from `animation`.
    static func serving(still: URL, animation: URL) -> FakeImageSource {
        FakeImageSource { request in
            if request.animated != nil {
                return ImageFile(
                    path: animation.path, width: 32, height: 32, animated: true, fromDisk: false)
            }
            return ImageFile(path: still.path, width: 80, height: 80, animated: false, fromDisk: false)
        }
    }

    /// The heavy GIF (`TestGIF.heavy`) for animated asks, `still` for the others.
    static func heavyGif(still: URL) async throws -> FakeImageSource {
        let gif = try await TestGIF.heavyURL
        return FakeImageSource { request in
            let path = request.animated == nil ? still.path : gif.path
            return ImageFile(
                path: path, width: 540, height: 405, animated: request.animated != nil, fromDisk: false)
        }
    }

    static func stalling() -> FakeImageSource {
        FakeImageSource { _ in
            try await Task.sleep(for: .seconds(60))
            throw ImageError.Unavailable
        }
    }

    static func failing() -> FakeImageSource {
        FakeImageSource { _ in throw ImageError.Unavailable }
    }

    func replace(_ answer: @escaping @Sendable (ImageRequest) async throws -> ImageFile) {
        self.answer.withLock { $0 = answer }
    }

    func image(_ request: ImageRequest) async throws -> ImageFile {
        requests.withLock { $0.append(request) }
        do {
            return try await answer.withLock { $0 }(request)
        } catch is CancellationError {
            cancelled.withLock { $0 += 1 }
            throw CancellationError()
        }
    }

    var count: Int { requests.withLock { $0.count } }
}

/// Fires once; waiters return at once afterwards.
final class Gate: Sendable {
    private struct State {
        var fired = false
        var waiting: [CheckedContinuation<Void, Never>] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    func fire() {
        let waiting = state.withLock { state in
            state.fired = true
            defer { state.waiting = [] }
            return state.waiting
        }
        waiting.forEach { $0.resume() }
    }

    func wait() async {
        await withCheckedContinuation { continuation in
            let fired = state.withLock { state in
                if !state.fired {
                    state.waiting.append(continuation)
                }
                return state.fired
            }
            if fired {
                continuation.resume()
            }
        }
    }
}

/// PNG files written for a test, deleted with it.
final class TestImages {
    let directory = FileManager.default.temporaryDirectory.appending(
        path: "akari-app-images-\(UUID().uuidString)", directoryHint: .isDirectory)

    init() {
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    deinit {
        try? FileManager.default.removeItem(at: directory)
    }

    /// A square PNG of `pixels` filled with `color` (RGBA 0–255).
    func png(_ name: String, pixels: Int, color: [Int] = [255, 0, 0, 255]) throws -> URL {
        try png(name, width: pixels, height: pixels, color: color)
    }

    /// A PNG of `width` × `height` filled with `color` (RGBA 0–255).
    func png(_ name: String, width: Int, height: Int, color: [Int] = [255, 0, 0, 255]) throws -> URL {
        let url = directory.appending(path: "\(name).png")
        let space = CGColorSpace(name: CGColorSpace.sRGB)!
        let context = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
            space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.setFillColor(
            CGColor(
                srgbRed: CGFloat(color[0]) / 255, green: CGFloat(color[1]) / 255,
                blue: CGFloat(color[2]) / 255, alpha: CGFloat(color[3]) / 255))
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        let destination = CGImageDestinationCreateWithURL(
            url as CFURL, UTType.png.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, context.makeImage()!, nil)
        guard CGImageDestinationFinalize(destination) else {
            throw CocoaError(.fileWriteUnknown)
        }
        return url
    }

    /// AkariKit's animated WebP fixture.
    var animated: URL {
        AnimatedFormatProbe.fixtureURL!
    }
}

extension ImageKey {
    nonisolated static func avatar(_ raw: UInt64, animated: Bool = false) -> ImageKey {
        .avatar(
            userId: UserId(rawValue: raw),
            hash: (animated ? "a_" : "") + "0123456789abcdef0123456789abcdef")
    }
}

/// Polls the main actor until `condition` holds, at most 5 s.
@MainActor
func eventually(
    _ condition: @MainActor () throws -> Bool, sourceLocation: SourceLocation = #_sourceLocation
) async throws {
    let deadline = ContinuousClock.now + .seconds(5)
    while ContinuousClock.now < deadline {
        if try condition() {
            return
        }
        try await Task.sleep(for: .milliseconds(10))
    }
    Issue.record("not within 5 s", sourceLocation: sourceLocation)
}

/// The frames `picture` shows over the next `count` changes of its frame, on the app's clock.
@MainActor
func frames(of picture: ImageLayerView, count: Int, sourceLocation: SourceLocation = #_sourceLocation)
    async throws -> [Int]
{
    var seen: [Int] = []
    let deadline = ContinuousClock.now + .seconds(20)
    while seen.count < count, ContinuousClock.now < deadline {
        if let frame = picture.player?.shownFrame, frame != seen.last {
            seen.append(frame)
        }
        try await Task.sleep(for: .milliseconds(5))
    }
    if seen.count < count {
        Issue.record("\(seen.count) frames within 20 s: \(seen)", sourceLocation: sourceLocation)
    }
    return seen
}

/// The images log's lines while `body` runs; suites running side by side each get every line.
@MainActor
func imageLog(_ body: () async throws -> Void) async rethrows -> [String] {
    let lines = OSAllocatedUnfairLock<[String]>(initialState: [])
    let id = ImageLogReaders.add { line in lines.withLock { $0.append(line) } }
    defer { ImageLogReaders.remove(id) }
    try await body()
    return lines.withLock { $0 }
}

@MainActor
private enum ImageLogReaders {
    private static var readers: [UUID: (String) -> Void] = [:]
    private static var before: (enabled: Bool, write: (String) -> Void)?

    static func add(_ reader: @escaping (String) -> Void) -> UUID {
        if readers.isEmpty {
            before = (ImageLog.enabled, ImageLog.write)
            ImageLog.enabled = true
            ImageLog.write = { line in readers.values.forEach { $0(line) } }
        }
        let id = UUID()
        readers[id] = reader
        return id
    }

    static func remove(_ id: UUID) {
        readers.removeValue(forKey: id)
        if readers.isEmpty, let before {
            ImageLog.enabled = before.enabled
            ImageLog.write = before.write
            self.before = nil
        }
    }
}

/// Discord's defaults, as the core gives them without the field.
nonisolated func displayDefaults() -> DisplaySettings {
    DisplaySettings(
        renderSpoilers: .onClick, viewImageDescriptions: false, inlineAttachmentMedia: true,
        inlineEmbedMedia: true, gifAutoPlay: true, renderEmbeds: true, renderReactions: true, animateEmoji: true,
        animateStickers: .always, explicitContentFilter: .nonFriends,
        explicitContent: .init(guilds: .unset, friendDm: .unset, nonFriendDm: .unset),
        gore: .init(guilds: .unset, friendDm: .unset, nonFriendDm: .unset),
        selfHarm: .init(guilds: .unset, friendDm: .unset, nonFriendDm: .unset))
}

/// A fake account's answer to every image request: nothing, as offline.
nonisolated final class UnavailableImage: PendingImage, @unchecked Sendable {
    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func result() async throws -> ImageFile {
        throw ImageError.Unavailable
    }

    override func cancel() {}
}
