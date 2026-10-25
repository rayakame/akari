import CoreGraphics
import Foundation
import ImageIO

/// ImageIO over the files the core hands out: a frame decoded so it covers a box's pixels,
/// never above the source's own size, with absurd sources refused before any decoding (D134).
public enum ImageDecoder {
    /// Sources with more pixels than this are a decompression bomb, not an image.
    public static let maxPixels = 64_000_000

    enum Failure: Error, Equatable {
        case missing
        case undecodable
        case tooLarge
    }

    /// Opens `url` and checks the first frame's size from the header against `maxPixels`.
    static func open(_ url: URL, maxPixels: Int = ImageDecoder.maxPixels) throws -> CGImageSource {
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw Failure.missing
        }
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
            CGImageSourceGetCount(source) > 0,
            let size = pixelSize(of: source, frame: 0)
        else {
            throw Failure.undecodable
        }
        if size.width * size.height > maxPixels {
            throw Failure.tooLarge
        }
        return source
    }

    static func pixelSize(of source: CGImageSource, frame: Int) -> (width: Int, height: Int)? {
        guard
            let properties = CGImageSourceCopyPropertiesAtIndex(source, frame, nil)
                as? [CFString: Any],
            let width = properties[kCGImagePropertyPixelWidth] as? Int,
            let height = properties[kCGImagePropertyPixelHeight] as? Int,
            width > 0, height > 0
        else {
            return nil
        }
        return (width, height)
    }

    /// `frame` decoded so that it covers `pixels` (the larger of the two ratios), at the
    /// source's own size when that is smaller: no image is drawn from fewer pixels than its box
    /// has, unless the source has fewer (D130).
    static func decode(
        _ source: CGImageSource, frame: Int, covering pixels: (width: Int, height: Int)
    ) -> CGImage? {
        guard let size = pixelSize(of: source, frame: frame) else {
            return nil
        }
        let longest = max(size.width, size.height)
        let scale = max(
            Double(pixels.width) / Double(size.width), Double(pixels.height) / Double(size.height))
        let wanted = scale >= 1 ? longest : Int((Double(longest) * scale).rounded(.up))
        let options: [CFString: Any] = [
            kCGImageSourceThumbnailMaxPixelSize: wanted,
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, frame, options as CFDictionary)
    }
}

/// An animated file's frames, decoded one at a time at a box's pixels; `nil` for a still.
public final class AnimationSource: @unchecked Sendable {
    public let url: URL
    public let frameCount: Int
    /// Each frame's time on screen, in seconds; delays under 10 ms read as 100 ms, as browsers
    /// show them.
    public let delays: [Double]
    /// 0 plays forever.
    public let loopCount: Int
    public let pixelSize: (width: Int, height: Int)
    private let source: CGImageSource
    // ImageIO's sources aren't for concurrent use.
    private let lock = NSLock()

    public init?(url: URL) {
        guard let source = try? ImageDecoder.open(url) else {
            return nil
        }
        let count = CGImageSourceGetCount(source)
        guard count > 1, let size = ImageDecoder.pixelSize(of: source, frame: 0) else {
            return nil
        }
        self.url = url
        self.source = source
        frameCount = count
        pixelSize = size
        let container = CGImageSourceCopyProperties(source, nil) as? [CFString: Any] ?? [:]
        delays = (0..<count).map { index in
            let properties =
                CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any] ?? [:]
            let delay = Self.delay(in: properties) ?? 0.1
            return delay < 0.01 ? 0.1 : delay
        }
        loopCount = Self.loopCount(in: container)
    }

    /// `index` decoded to cover `pixels`.
    public func frame(_ index: Int, pixels: (width: Int, height: Int)) -> CGImage? {
        guard (0..<frameCount).contains(index) else {
            return nil
        }
        return lock.withLock {
            ImageDecoder.decode(source, frame: index, covering: pixels)
        }
    }

    /// One pass through every frame.
    public var cycle: Double {
        delays.reduce(0, +)
    }

    private static func delay(in properties: [CFString: Any]) -> Double? {
        let formats: [(CFString, CFString, CFString)] = [
            (
                kCGImagePropertyWebPDictionary, kCGImagePropertyWebPUnclampedDelayTime,
                kCGImagePropertyWebPDelayTime
            ),
            (
                kCGImagePropertyGIFDictionary, kCGImagePropertyGIFUnclampedDelayTime,
                kCGImagePropertyGIFDelayTime
            ),
            (
                kCGImagePropertyPNGDictionary, kCGImagePropertyAPNGUnclampedDelayTime,
                kCGImagePropertyAPNGDelayTime
            ),
        ]
        for (dictionary, unclamped, clamped) in formats {
            if let format = properties[dictionary] as? [CFString: Any] {
                return (format[unclamped] ?? format[clamped]) as? Double
            }
        }
        return nil
    }

    // Without a loop count a GIF or APNG plays once; WebP's ANIM chunk always has one.
    private static func loopCount(in container: [CFString: Any]) -> Int {
        let formats: [(CFString, CFString)] = [
            (kCGImagePropertyWebPDictionary, kCGImagePropertyWebPLoopCount),
            (kCGImagePropertyGIFDictionary, kCGImagePropertyGIFLoopCount),
            (kCGImagePropertyPNGDictionary, kCGImagePropertyAPNGLoopCount),
        ]
        for (dictionary, key) in formats {
            if let format = container[dictionary] as? [CFString: Any] {
                return format[key] as? Int ?? 1
            }
        }
        return 1
    }
}
