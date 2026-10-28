import CoreGraphics
import Foundation

/// Discord's placeholders (thumbhashes, at most 32 px) as images, decoded once each; the oldest go
/// past `limit`.
@MainActor
public final class PlaceholderCache {
    private let limit: Int
    private var images: [String: CGImage] = [:]
    // Oldest first.
    private var order: [String] = []

    public init(limit: Int = 512) {
        self.limit = limit
    }

    public var count: Int { images.count }

    public func image(_ thumbhash: String) -> CGImage? {
        if let image = images[thumbhash] {
            return image
        }
        guard let decoded = placeholderImage(thumbhash: thumbhash),
            let image = Self.image(decoded)
        else {
            return nil
        }
        images[thumbhash] = image
        order.append(thumbhash)
        if order.count > limit {
            images.removeValue(forKey: order.removeFirst())
        }
        return image
    }

    private static func image(_ placeholder: PlaceholderImage) -> CGImage? {
        let width = Int(placeholder.width)
        let height = Int(placeholder.height)
        guard width > 0, height > 0, placeholder.rgba.count == width * height * 4,
            let provider = CGDataProvider(data: Data(placeholder.rgba) as CFData)
        else {
            return nil
        }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue), provider: provider,
            decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }
}
