import CoreGraphics
import Foundation
import ImageIO

/// Which form animations are asked in on this Mac: WebP where ImageIO composites animated WebP
/// right, GIF where it doesn't (macOS 14 fills a frame disposed to the background with the frame
/// before). Decided once at launch from a fixture shipped with AkariKit, off the main actor.
public enum AnimatedFormatProbe {
    /// Five 32 × 32 frames; frame 1 is disposed to the background, so frame 2's left half must
    /// read clear (`Tests/AkariKitTests/Fixtures/Images/make.sh`).
    public static var fixtureURL: URL? {
        Bundle.module.url(forResource: "animated", withExtension: "webp", subdirectory: "Resources")
    }

    public static func run() async -> AnimatedFormat {
        guard let url = fixtureURL else {
            return .gif
        }
        return await Task.detached(priority: .utility) { probe(url) }.value
    }

    /// Decodes frame 2 at 16 pixels and reads its left half.
    static func probe(_ url: URL) -> AnimatedFormat {
        guard FileManager.default.fileExists(atPath: url.path),
            let source = CGImageSourceCreateWithURL(url as CFURL, nil),
            CGImageSourceGetCount(source) > 2
        else {
            return .gif
        }
        let options: [CFString: Any] = [
            kCGImageSourceThumbnailMaxPixelSize: 16,
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let frame = CGImageSourceCreateThumbnailAtIndex(source, 2, options as CFDictionary)
        else {
            return .gif
        }
        return decide(leftHalf: pixel(of: frame, x: 4, y: 8))
    }

    /// `leftHalf` is frame 2's premultiplied RGBA at the left half's center; clear is right.
    static func decide(leftHalf: [Int]?) -> AnimatedFormat {
        guard let leftHalf, leftHalf.count == 4 else {
            return .gif
        }
        return leftHalf.allSatisfy { $0 <= 3 } ? .webP : .gif
    }

    private static func pixel(of image: CGImage, x: Int, y: Int) -> [Int]? {
        guard x < image.width, y < image.height,
            let space = CGColorSpace(name: CGColorSpace.sRGB)
        else {
            return nil
        }
        var bytes = [UInt8](repeating: 0, count: image.width * image.height * 4)
        let drawn = bytes.withUnsafeMutableBytes { buffer -> Bool in
            guard
                let context = CGContext(
                    data: buffer.baseAddress, width: image.width, height: image.height,
                    bitsPerComponent: 8, bytesPerRow: image.width * 4, space: space,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else {
                return false
            }
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            return true
        }
        guard drawn else {
            return nil
        }
        let start = (y * image.width + x) * 4
        return bytes[start..<start + 4].map(Int.init)
    }
}
