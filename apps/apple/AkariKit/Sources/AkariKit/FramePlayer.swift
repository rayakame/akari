import CoreGraphics
import Foundation
import QuartzCore

/// One clock for every animation: a 60 Hz timer that runs only while a player is registered,
/// ticking each player with the same time. Tests drive a manual one with `advance(to:)`.
@MainActor
public final class AnimationClock {
    public static let shared = AnimationClock()

    private struct Weak {
        weak var player: FramePlayer?
    }

    private let automatic: Bool
    private var players: [ObjectIdentifier: Weak] = [:]
    private var timer: Timer?
    private var manualNow: Double = 0
    private static let interval: TimeInterval = 1.0 / 60.0

    public init(automatic: Bool = true) {
        self.automatic = automatic
    }

    public var playerCount: Int {
        players.values.filter { $0.player != nil }.count
    }

    public var isRunning: Bool {
        timer != nil
    }

    /// Seconds, on the media timeline (or the last `advance` of a manual clock).
    public var now: Double {
        automatic ? CACurrentMediaTime() : manualNow
    }

    /// A manual clock's tick: every player sees `seconds`.
    public func advance(to seconds: Double) {
        manualNow = seconds
        tick(seconds)
    }

    func register(_ player: FramePlayer) {
        players[ObjectIdentifier(player)] = Weak(player: player)
        if automatic, timer == nil {
            let timer = Timer(timeInterval: Self.interval, repeats: true) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self else {
                        return
                    }
                    self.tick(self.now)
                }
            }
            timer.tolerance = Self.interval / 4
            // Common modes, so animations run while the list scrolls.
            RunLoop.main.add(timer, forMode: .common)
            self.timer = timer
        }
    }

    func unregister(_ player: FramePlayer) {
        players.removeValue(forKey: ObjectIdentifier(player))
        prune()
    }

    private func tick(_ now: Double) {
        for entry in players.values {
            entry.player?.tick(now)
        }
        prune()
    }

    private func prune() {
        players = players.filter { $0.value.player != nil }
        if players.isEmpty {
            timer?.invalidate()
            timer = nil
        }
    }
}

/// Plays an `AnimationSource` into a callback: each frame decoded at the box's pixels off the
/// main actor, shown at its delay, loops honoured, the last frame kept when the loops are up. A
/// tick never waits for a frame: one not decoded yet shows as soon as it is, if still due.
@MainActor
public final class FramePlayer {
    public typealias Frame = @MainActor (CGImage, Int) -> Void

    /// Frames kept decoded per player; past it, only a window ahead of the play head.
    nonisolated public static let frameBudget = 24 << 20
    nonisolated private static let window = 4

    public let animation: AnimationSource
    public let pixels: (width: Int, height: Int)
    public private(set) var isPlaying = false
    private let clock: AnimationClock
    private let onFrame: Frame
    private var frames: [Int: CGImage] = [:]
    private var started: Double?
    private var shownIndex: Int?
    private var dueIndex = 0
    private var keepsAll: Bool
    private var generation = 0
    // What the decoder shares with the player: where the play head is, which frames are here,
    // and its own task, which goes with the player.
    private let head = LockedBox(0)
    private let present = LockedBox(Set<Int>())
    private let decoding = LockedBox<Task<Void, Never>?>(nil)

    public init(
        animation: AnimationSource, pixels: (width: Int, height: Int),
        clock: AnimationClock = .shared, frameBudget: Int = FramePlayer.frameBudget,
        onFrame: @escaping Frame
    ) {
        self.animation = animation
        self.pixels = pixels
        self.clock = clock
        self.onFrame = onFrame
        let perFrame = pixels.width * pixels.height * 4
        keepsAll = perFrame * animation.frameCount <= frameBudget
    }

    deinit {
        decoding.current?.cancel()
    }

    var isDecoding: Bool {
        decoding.current != nil
    }

    var decodedFrameCount: Int {
        frames.count
    }

    /// Decodes the frames ahead of a start (all of them when they fit the budget), so the first
    /// ticks have them.
    public func prepare() async {
        let wanted = keepsAll ? 0..<animation.frameCount : 0..<min(Self.window, animation.frameCount)
        let indexes = wanted.filter { frames[$0] == nil }
        guard !indexes.isEmpty else {
            return
        }
        let animation = animation
        let pixels = pixels
        let decoded = await Task.detached(priority: .userInitiated) {
            indexes.compactMap { index in animation.frame(index, pixels: pixels).map { (index, $0) } }
        }.value
        for (index, image) in decoded where frames[index] == nil {
            frames[index] = image
        }
        present.withLock { $0 = Set(frames.keys) }
        if isPlaying {
            show(dueIndex)
        }
    }

    /// Starts from frame 0 at `time` (the clock's now when `nil`).
    public func start(at time: Double? = nil) {
        guard !isPlaying else {
            return
        }
        isPlaying = true
        started = time ?? clock.now
        shownIndex = nil
        dueIndex = 0
        head.withLock { $0 = 0 }
        clock.register(self)
        show(0)
        startDecoding()
    }

    public func stop() {
        guard isPlaying else {
            return
        }
        isPlaying = false
        started = nil
        stopDecoding()
        generation += 1
        clock.unregister(self)
        if !keepsAll {
            frames = [:]
            present.withLock { $0 = [] }
        }
    }

    func tick(_ now: Double) {
        guard isPlaying, let started else {
            return
        }
        let elapsed = max(0, now - started)
        let cycle = animation.cycle
        guard cycle > 0 else {
            return
        }
        let loops = Int(elapsed / cycle)
        if animation.loopCount > 0, loops >= animation.loopCount {
            show(animation.frameCount - 1)
            stop()
            return
        }
        var within = elapsed - Double(loops) * cycle
        var index = 0
        for (frame, delay) in animation.delays.enumerated() {
            within -= delay
            if within < -1e-9 {
                index = frame
                break
            }
            index = min(frame + 1, animation.frameCount - 1)
        }
        if index != dueIndex {
            dueIndex = index
            head.withLock { $0 = index }
            show(index)
        }
    }

    // Shows `index` if it's decoded and not the frame on screen.
    private func show(_ index: Int) {
        guard shownIndex != index, let image = frames[index] else {
            return
        }
        shownIndex = index
        onFrame(image, index)
    }

    private func startDecoding() {
        stopDecoding()
        let animation = animation
        let pixels = pixels
        let count = animation.frameCount
        let generation = generation
        if keepsAll {
            // Every frame once; nothing to do when `prepare` or an earlier start got them all.
            let missing = (0..<count).filter { frames[$0] == nil }
            guard !missing.isEmpty else {
                return
            }
            decoding.withLock { $0 = Task.detached(priority: .userInitiated) { [weak self] in
                for index in missing where !Task.isCancelled {
                    if let image = animation.frame(index, pixels: pixels) {
                        await self?.decoded(index, image, generation: generation)
                    }
                }
                await self?.decodingEnded()
            } }
            return
        }
        let head = head
        let present = present
        decoding.withLock { $0 = Task.detached(priority: .userInitiated) { [weak self] in
            // A window ahead of the play head; frames behind it are dropped by the player.
            var next = 0
            while !Task.isCancelled {
                let playHead = head.current
                if (next - playHead + count) % count >= Self.window {
                    next = playHead
                    try? await Task.sleep(for: .milliseconds(16))
                    continue
                }
                let index = next % count
                next += 1
                if present.current.contains(index) {
                    continue
                }
                if let image = animation.frame(index, pixels: pixels) {
                    await self?.decoded(index, image, generation: generation)
                }
            }
        } }
    }

    private func stopDecoding() {
        decoding.withLock { task in
            task?.cancel()
            task = nil
        }
    }

    private func decodingEnded() {
        decoding.withLock { $0 = nil }
    }

    private func decoded(_ index: Int, _ image: CGImage, generation: Int) {
        guard generation == self.generation else {
            return
        }
        frames[index] = image
        if !keepsAll {
            let count = animation.frameCount
            let playHead = head.current
            frames = frames.filter { (($0.key - playHead + count) % count) < Self.window }
        }
        present.withLock { $0 = Set(frames.keys) }
        if isPlaying, index == dueIndex {
            show(index)
        }
    }
}

// A value behind a lock, for what the decoder and the player share.
final class LockedBox<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: Value

    init(_ value: Value) {
        self.value = value
    }

    var current: Value { lock.withLock { value } }

    func withLock<R>(_ body: (inout Value) -> R) -> R {
        lock.withLock { body(&value) }
    }
}
