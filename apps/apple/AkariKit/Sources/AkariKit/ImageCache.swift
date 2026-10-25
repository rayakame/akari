import CoreGraphics
import Foundation

/// Where the cache gets its files: `Account`, or a fake in tests.
public protocol ImageSource: Sendable {
    func image(_ request: ImageRequest) async throws -> ImageFile
}

extension Account: ImageSource {}

/// One box's ask: the image, the box's pixels (points times the backing scale, rounded up) and
/// whether it should move.
public struct ImageSpec: Hashable, Sendable {
    public var key: ImageKey
    public var width: Int
    public var height: Int
    /// `nil` asks for the still.
    public var animated: AnimatedFormat?

    public init(key: ImageKey, width: Int, height: Int, animated: AnimatedFormat? = nil) {
        self.key = key
        self.width = width
        self.height = height
        self.animated = animated
    }

    public var request: ImageRequest {
        ImageRequest(
            key: key, width: UInt32(clamping: max(width, 1)),
            height: UInt32(clamping: max(height, 1)), animated: animated)
    }

    var pixels: (width: Int, height: Int) {
        (max(width, 1), max(height, 1))
    }
}

/// The two decoded tiers: identity images (avatars, icons, emoji, stickers) and a message's
/// media, each bounded in bytes on its own.
public enum ImageTier: Sendable, CaseIterable {
    case identity
    case media
}

public enum ImageCacheError: Error, Equatable {
    /// More pixels than Akari decodes (64 MP).
    case tooLarge
    /// Not an image ImageIO reads, or a file gone twice.
    case undecodable
}

/// Decoded images for the boxes on screen (D134): each file decoded once at its box's pixels off
/// the main actor, kept in two LRUs bounded in bytes, pinned while a box shows it. Asks for one
/// spec share a single source call, which the last waiter's cancellation cancels.
@MainActor
public final class ImageCache {
    public static let identityLimit = 16 << 20
    public static let mediaLimit = 48 << 20

    private let source: any ImageSource
    private let maxPixels: Int
    private var tiers: [ImageTier: Tier]
    private var pins: [ImageSpec: Int] = [:]
    private var inflight: [ImageSpec: Inflight] = [:]
    nonisolated(unsafe) private var pressure: DispatchSourceMemoryPressure?

    private struct Entry {
        let image: CGImage
        let bytes: Int
    }

    private struct Tier {
        let limit: Int
        var entries: [ImageSpec: Entry] = [:]
        // Least recently used first.
        var order: [ImageSpec] = []
        var bytes = 0

        mutating func touch(_ spec: ImageSpec) {
            if let index = order.firstIndex(of: spec) {
                order.remove(at: index)
                order.append(spec)
            }
        }

        mutating func remove(_ spec: ImageSpec) {
            if let entry = entries.removeValue(forKey: spec) {
                bytes -= entry.bytes
                order.removeAll { $0 == spec }
            }
        }
    }

    private struct Inflight {
        let task: Task<CGImage, Error>
        var waiters: [UUID: CheckedContinuation<CGImage, Error>]
    }

    public init(
        source: any ImageSource, identityLimit: Int = ImageCache.identityLimit,
        mediaLimit: Int = ImageCache.mediaLimit, maxPixels: Int = ImageDecoder.maxPixels
    ) {
        self.source = source
        self.maxPixels = maxPixels
        tiers = [.identity: Tier(limit: identityLimit), .media: Tier(limit: mediaLimit)]
        let pressure = DispatchSource.makeMemoryPressureSource(
            eventMask: [.warning, .critical], queue: .main)
        pressure.setEventHandler { [weak self] in
            MainActor.assumeIsolated { self?.dropUnpinned() }
        }
        pressure.activate()
        self.pressure = pressure
    }

    deinit {
        pressure?.cancel()
    }

    /// A decoded image already here, moved to the front of its tier; nothing is asked.
    public func cached(_ spec: ImageSpec) -> CGImage? {
        for tier in ImageTier.allCases {
            if let entry = tiers[tier]?.entries[spec] {
                tiers[tier]?.touch(spec)
                return entry.image
            }
        }
        return nil
    }

    /// The still image for `spec`, decoded at its pixels and kept in `tier`. Cancelling the
    /// task leaves the shared load to its other waiters, and cancels it with the last one.
    public func image(_ spec: ImageSpec, tier: ImageTier = .identity) async throws -> CGImage {
        if let image = cached(spec) {
            return image
        }
        let waiter = UUID()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                if Task.isCancelled {
                    return continuation.resume(throwing: CancellationError())
                }
                if inflight[spec] == nil {
                    start(spec, tier: tier)
                }
                inflight[spec]?.waiters[waiter] = continuation
            }
        } onCancel: {
            Task { @MainActor [weak self] in self?.leave(spec, waiter: waiter) }
        }
    }

    /// The frames of an animated image; the player decodes them at its box's pixels.
    public func animation(_ spec: ImageSpec) async throws -> AnimationSource {
        let source = self.source
        let request = spec.request
        return try await Task.detached(priority: .userInitiated) {
            let file = try await source.image(request)
            guard let animation = AnimationSource(url: URL(fileURLWithPath: file.path)) else {
                let again = try await source.image(request)
                guard let animation = AnimationSource(url: URL(fileURLWithPath: again.path))
                else {
                    throw ImageCacheError.undecodable
                }
                return animation
            }
            return animation
        }.value
    }

    /// Keeps the entry while a box shows it; balanced by `unpin`.
    public func pin(_ spec: ImageSpec) {
        pins[spec, default: 0] += 1
    }

    public func unpin(_ spec: ImageSpec) {
        guard let count = pins[spec] else {
            return
        }
        if count <= 1 {
            pins.removeValue(forKey: spec)
        } else {
            pins[spec] = count - 1
        }
    }

    /// A memory warning: everything not on screen goes.
    public func dropUnpinned() {
        for tier in ImageTier.allCases {
            drop(tier)
        }
    }

    /// The window is hidden or minimized: media not on screen goes, identity images stay.
    public func dropMedia() {
        drop(.media)
    }

    public func isPinned(_ spec: ImageSpec) -> Bool {
        pins[spec] != nil
    }

    public var identityBytes: Int { tiers[.identity]?.bytes ?? 0 }
    public var mediaBytes: Int { tiers[.media]?.bytes ?? 0 }
    public var inflightCount: Int { inflight.count }

    private func drop(_ tier: ImageTier) {
        guard var entries = tiers[tier] else {
            return
        }
        for spec in entries.order where pins[spec] == nil {
            entries.remove(spec)
        }
        tiers[tier] = entries
    }

    private func start(_ spec: ImageSpec, tier: ImageTier) {
        let source = self.source
        let pixels = spec.pixels
        let request = spec.request
        let maxPixels = maxPixels
        let task = Task.detached(priority: .userInitiated) {
            try await Self.load(request, pixels: pixels, maxPixels: maxPixels, from: source)
        }
        inflight[spec] = Inflight(task: task, waiters: [:])
        Task { [weak self] in
            let outcome = await task.result
            self?.finished(spec, task: task, tier: tier, outcome: outcome)
        }
    }

    // A waiter whose task was cancelled stops waiting; the last one to go cancels the load.
    private func leave(_ spec: ImageSpec, waiter: UUID) {
        guard var running = inflight[spec],
            let continuation = running.waiters.removeValue(forKey: waiter)
        else {
            return
        }
        continuation.resume(throwing: CancellationError())
        if running.waiters.isEmpty {
            running.task.cancel()
            inflight.removeValue(forKey: spec)
        } else {
            inflight[spec] = running
        }
    }

    private func finished(
        _ spec: ImageSpec, task: Task<CGImage, Error>, tier: ImageTier,
        outcome: Result<CGImage, Error>
    ) {
        guard let running = inflight[spec], running.task == task else {
            return
        }
        inflight.removeValue(forKey: spec)
        if case .success(let image) = outcome {
            keep(image, for: spec, in: tier)
        }
        for continuation in running.waiters.values {
            continuation.resume(with: outcome)
        }
    }

    private func keep(_ image: CGImage, for spec: ImageSpec, in tier: ImageTier) {
        guard var entries = tiers[tier] else {
            return
        }
        let bytes = image.bytesPerRow * image.height
        entries.remove(spec)
        // Larger than the whole tier: handed out, not kept.
        guard bytes <= entries.limit else {
            tiers[tier] = entries
            return
        }
        entries.entries[spec] = Entry(image: image, bytes: bytes)
        entries.order.append(spec)
        entries.bytes += bytes
        var index = 0
        while entries.bytes > entries.limit, index < entries.order.count {
            let oldest = entries.order[index]
            if pins[oldest] == nil, oldest != spec {
                entries.remove(oldest)
            } else {
                index += 1
            }
        }
        tiers[tier] = entries
    }

    // Off the main actor: the file, then the decode. A file gone between the answer and the
    // decode (the system purged the caches) is asked for once more.
    private static func load(
        _ request: ImageRequest, pixels: (width: Int, height: Int), maxPixels: Int,
        from source: any ImageSource
    ) async throws -> CGImage {
        var file = try await source.image(request)
        for attempt in 0..<2 {
            do {
                let opened = try ImageDecoder.open(
                    URL(fileURLWithPath: file.path), maxPixels: maxPixels)
                guard let image = ImageDecoder.decode(opened, frame: 0, covering: pixels) else {
                    throw ImageCacheError.undecodable
                }
                return image
            } catch ImageDecoder.Failure.missing where attempt == 0 {
                try Task.checkCancellation()
                file = try await source.image(request)
            } catch ImageDecoder.Failure.missing {
                throw ImageCacheError.undecodable
            } catch ImageDecoder.Failure.tooLarge {
                throw ImageCacheError.tooLarge
            } catch ImageDecoder.Failure.undecodable {
                throw ImageCacheError.undecodable
            }
        }
        throw ImageCacheError.undecodable
    }
}
