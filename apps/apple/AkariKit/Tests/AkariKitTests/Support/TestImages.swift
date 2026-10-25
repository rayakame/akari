import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

@testable import AkariKit

// Image files written for a test, in a folder of their own, deleted with it.
final class TestImages: @unchecked Sendable {
    let directory: URL

    init() {
        directory = FileManager.default.temporaryDirectory.appending(
            path: "akari-test-images-\(UUID().uuidString)", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    deinit {
        try? FileManager.default.removeItem(at: directory)
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
        let image = context.makeImage()!
        let destination = CGImageDestinationCreateWithURL(
            url as CFURL, UTType.png.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else {
            throw CocoaError(.fileWriteUnknown)
        }
        return url
    }

    var animatedWebP: URL {
        AnimatedFormatProbe.fixtureURL!
    }
}

/// An image source for tests: answers from a script, records every request and every
/// cancellation that reached it.
final class FakeImageSource: ImageSource, @unchecked Sendable {
    let requests = Locked<[ImageRequest]>([])
    let cancelled = Locked(0)
    let started = AsyncQueue<ImageRequest>()
    private let answer: Locked<@Sendable (ImageRequest) async throws -> ImageFile>

    init(_ answer: @escaping @Sendable (ImageRequest) async throws -> ImageFile) {
        self.answer = Locked(answer)
    }

    /// Answers every request with the file at `url`.
    static func serving(_ url: URL, width: UInt32, height: UInt32, animated: Bool = false)
        -> FakeImageSource
    {
        FakeImageSource { _ in
            ImageFile(
                path: url.path, width: width, height: height, animated: animated, fromDisk: false)
        }
    }

    /// Never answers; a cancelled ask is counted.
    static func stalling() -> FakeImageSource {
        FakeImageSource { _ in
            try await Task.sleep(for: .seconds(60))
            throw ImageError.Unavailable
        }
    }

    func replaceAnswer(_ answer: @escaping @Sendable (ImageRequest) async throws -> ImageFile) {
        self.answer.withLock { $0 = answer }
    }

    func image(_ request: ImageRequest) async throws -> ImageFile {
        requests.withLock { $0.append(request) }
        started.send(request)
        do {
            return try await answer.current(request)
        } catch is CancellationError {
            cancelled.withLock { $0 += 1 }
            throw CancellationError()
        }
    }
}

extension ImageKey {
    static func testAvatar(_ raw: UInt64) -> ImageKey {
        .avatar(userId: UserId(rawValue: raw), hash: "0123456789abcdef0123456789abcdef")
    }
}
