import CoreGraphics
import Foundation
import ImageIO
import Testing

// What Akari relies on ImageIO for with animated WebP, the CDN's animated form. Self-contained
// (no AkariKit), so it also runs alone on the deployment target's macOS.
@Suite(.timeLimit(.minutes(1)))
struct ImageIOWebPTests {
    // Fixtures/Images/make.sh: 32 × 32, five frames, three loops, lossless.
    static let delays: [Double] = [0.1, 0.2, 0.05, 0.3, 0.15]

    // Premultiplied RGBA at the left and right halves' centers.
    static let frames: [(left: [Int], right: [Int])] = [
        ([255, 0, 0, 255], [255, 0, 0, 255]),
        ([0, 255, 0, 255], [0, 255, 0, 255]),
        ([0, 0, 0, 0], [0, 0, 255, 255]),
        ([128, 128, 0, 128], [128, 128, 0, 128]),
        // Stored as its left half over the frame before.
        ([255, 0, 255, 255], [128, 128, 0, 128]),
    ]

    static func url() throws -> URL {
        try #require(Bundle.module.url(
            forResource: "animated", withExtension: "webp", subdirectory: "Fixtures/Images"))
    }

    static func source() throws -> CGImageSource {
        try #require(CGImageSourceCreateWithURL(url() as CFURL, nil))
    }

    @Test
    func opensWithItsFramesDelaysAndLoops() throws {
        let source = try Self.source()

        #expect(CGImageSourceGetType(source) as String? == "org.webmproject.webp")
        #expect(CGImageSourceGetCount(source) == Self.delays.count)
        for (index, delay) in Self.delays.enumerated() {
            let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any]
            let webp = properties?[kCGImagePropertyWebPDictionary] as? [CFString: Any]
            let unclamped = webp?[kCGImagePropertyWebPUnclampedDelayTime] as? Double
            #expect(unclamped.map { abs($0 - delay) < 0.001 } == true, "frame \(index): \(String(describing: unclamped))")
        }
        let container = CGImageSourceCopyProperties(source, nil) as? [CFString: Any]
        let webp = container?[kCGImagePropertyWebPDictionary] as? [CFString: Any]
        #expect(webp?[kCGImagePropertyWebPLoopCount] as? Int == 3)
    }

    @Test
    func framesDecodeComposited() throws {
        let source = try Self.source()

        for index in Self.frames.indices {
            let image = try #require(CGImageSourceCreateImageAtIndex(source, index, nil))
            #expect(image.width == 32 && image.height == 32, "frame \(index)")
            let pixels = try Self.pixels(image)
            Self.expectFrame(index, left: pixels.at(x: 8, y: 16), right: pixels.at(x: 24, y: 16))
        }
    }

    // Akari plays animations itself from these thumbnails: each frame whole, at the box's pixels.
    @Test
    func everyFrameDecodesCompositedAtTheBoxPixels() throws {
        let source = try Self.source()
        let options: [CFString: Any] = [
            kCGImageSourceThumbnailMaxPixelSize: 16,
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceShouldCacheImmediately: true,
        ]

        for index in Self.frames.indices {
            let thumbnail = try #require(
                CGImageSourceCreateThumbnailAtIndex(source, index, options as CFDictionary))
            #expect(thumbnail.width == 16 && thumbnail.height == 16, "frame \(index)")
            let pixels = try Self.pixels(thumbnail)
            Self.expectFrame(index, left: pixels.at(x: 4, y: 8), right: pixels.at(x: 12, y: 8))
        }
    }

    struct Pixels {
        let bytes: [UInt8]
        let width: Int

        func at(x: Int, y: Int) -> [Int] {
            let start = (y * width + x) * 4
            return bytes[start..<start + 4].map(Int.init)
        }
    }

    static func pixels(_ image: CGImage) throws -> Pixels {
        var bytes = [UInt8](repeating: 0, count: image.width * image.height * 4)
        let space = try #require(CGColorSpace(name: CGColorSpace.sRGB))
        try bytes.withUnsafeMutableBytes { buffer in
            let context = try #require(CGContext(
                data: buffer.baseAddress, width: image.width, height: image.height,
                bitsPerComponent: 8, bytesPerRow: image.width * 4, space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        }
        return Pixels(bytes: bytes, width: image.width)
    }

    // macOS 14's ImageIO fills frame 1's dispose-to-background area with frame 0 instead of
    // clearing it (seen on 14.8.9; 15.7 and 26 clear it). Intermittent: a decoder that happens
    // to get it right on one run doesn't fail the test.
    static func expectFrame(
        _ index: Int, left: [Int], right: [Int], sourceLocation: SourceLocation = #_sourceLocation
    ) {
        let expected = frames[index]
        withKnownIssue("macOS 14 disposes frame 1 to frame 0", isIntermittent: true) {
            expectColor(left, expected.left, "frame \(index), left", sourceLocation: sourceLocation)
        } when: {
            index == 2 && ProcessInfo.processInfo.operatingSystemVersion.majorVersion == 14
        }
        expectColor(right, expected.right, "frame \(index), right", sourceLocation: sourceLocation)
    }

    static func expectColor(
        _ actual: [Int], _ expected: [Int], _ comment: String,
        sourceLocation: SourceLocation = #_sourceLocation
    ) {
        let close = zip(actual, expected).allSatisfy { abs($0 - $1) <= 3 }
        #expect(close, "\(comment): \(actual), expected \(expected)", sourceLocation: sourceLocation)
    }
}
