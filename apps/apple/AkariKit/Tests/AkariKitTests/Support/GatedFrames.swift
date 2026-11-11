import CoreGraphics
import Foundation

@testable import AkariKit

/// Frames a test lets through the decoder one at a time, as a decoder slower than the frames'
/// delays would hand them over.
final class GatedFrames: FrameSource, @unchecked Sendable {
    let frameCount: Int
    let delays: [Double]
    let loopCount: Int
    /// Every index asked for, in order.
    let decoded = Locked<[Int]>([])
    private let broken: Set<Int>
    private let image: CGImage
    private let gate = NSCondition()
    private var allowed = 0

    init(count: Int, delay: Double, loopCount: Int = 0, broken: Set<Int> = []) {
        frameCount = count
        delays = Array(repeating: delay, count: count)
        self.loopCount = loopCount
        self.broken = broken
        let context = CGContext(
            data: nil, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        image = context.makeImage()!
    }

    var cycle: Double { delays.reduce(0, +) }

    /// Lets `count` more decodes finish.
    func allow(_ count: Int = 1) {
        gate.withLock {
            allowed = count == .max ? .max : allowed + count
            gate.broadcast()
        }
    }

    func frame(_ index: Int, pixels: (width: Int, height: Int), fit: ImageFit) -> CGImage? {
        gate.lock()
        // A cancelled decoder gives its thread back.
        while allowed == 0, !Task.isCancelled {
            gate.wait(until: Date(timeIntervalSinceNow: 0.01))
        }
        let passed = allowed > 0
        if passed, allowed != .max {
            allowed -= 1
        }
        gate.unlock()
        guard passed else {
            return nil
        }
        decoded.withLock { $0.append(index) }
        return broken.contains(index) ? nil : image
    }
}
