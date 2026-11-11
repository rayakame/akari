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

/// Plays a `FrameSource` into a callback: each frame decoded at the box's pixels off the main
/// actor, shown at its delay, loops honoured, the last frame kept when the loops are up.
///
/// Frames that fit the budget are all kept, and the clock alone says which shows: a tick never
/// waits for a frame, one not decoded yet shows as soon as it is, if still due. Past the budget
/// only a window of frames ahead of the picture is kept, decoded in order, and the picture never
/// passes a frame that isn't decoded yet: a decoder slower than the delays plays slow, in order.
@MainActor
public final class FramePlayer {
    public typealias Frame = @MainActor (CGImage, Int) -> Void

    /// Frames kept decoded per player; past it, only a window ahead of the picture.
    nonisolated public static let frameBudget = 24 << 20
    nonisolated static let window = 4

    public let animation: any FrameSource
    public let pixels: (width: Int, height: Int)
    public let fit: ImageFit
    public private(set) var isPlaying = false
    private let clock: AnimationClock
    private let onFrame: Frame
    private let keepsAll: Bool
    private var frames: [Int: CGImage] = [:]
    // Frames that didn't decode: passed over, never waited for.
    private var broken: Set<Int> = []
    private var started: Double?
    private var shownIndex: Int?
    private var dueIndex = 0
    // A window's play head: the frame to show next, when it's due on the clock, the loops
    // played, and whether the picture stands waiting for that frame.
    private var nextIndex = 0
    private var nextDue = 0.0
    private var loopsDone = 0
    private var waiting = false
    private var generation = 0
    // What the decoder shares with the player: where the window starts, which of its frames
    // need no decoding, its wake-up, and its own task, which goes with the player.
    private let head = LockedBox(0)
    private let settled = LockedBox(Set<Int>())
    private let wake = LockedBox<AsyncStream<Void>.Continuation?>(nil)
    private let decoding = LockedBox<Task<Void, Never>?>(nil)

    public init(
        animation: any FrameSource, pixels: (width: Int, height: Int), fit: ImageFit = .cover,
        clock: AnimationClock = .shared, frameBudget: Int = FramePlayer.frameBudget,
        onFrame: @escaping Frame
    ) {
        self.animation = animation
        self.pixels = pixels
        self.fit = fit
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

    /// The frame on screen, `nil` before the first.
    public var shownFrame: Int? {
        shownIndex
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
        let fit = fit
        let decoded = await Task.detached(priority: .userInitiated) {
            indexes.compactMap { index in animation.frame(index, pixels: pixels, fit: fit).map { (index, $0) } }
        }.value
        for (index, image) in decoded where frames[index] == nil {
            frames[index] = image
        }
        if !keepsAll, isPlaying {
            advance(clock.now, landed: true)
            return
        }
        publish()
        if isPlaying {
            show(dueIndex)
        }
    }

    /// Starts from frame 0 at `time` (the clock's now when `nil`).
    public func start(at time: Double? = nil) {
        guard !isPlaying else {
            return
        }
        let time = time ?? clock.now
        isPlaying = true
        started = time
        shownIndex = nil
        dueIndex = 0
        nextIndex = 0
        nextDue = time
        loopsDone = 0
        waiting = false
        head.withLock { $0 = 0 }
        clock.register(self)
        if keepsAll {
            show(0)
        } else {
            advance(time)
        }
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
            publish()
        }
    }

    func tick(_ now: Double) {
        guard isPlaying, let started else {
            return
        }
        guard keepsAll else {
            advance(now)
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
            show(index)
        }
    }

    // A window's tick: the picture moves to the newest decoded frame that's due, through the
    // frames before it, and stops at the first that isn't decoded. That one shows when it
    // lands, and the frames after it take their delays from then.
    private func advance(_ now: Double, landed: Bool = false) {
        let count = animation.frameCount
        let first = nextIndex
        var reached: Int?
        var steps = 0
        while isPlaying, steps < count, now >= nextDue - 1e-9 {
            if nextIndex == 0, animation.loopCount > 0, loopsDone >= animation.loopCount {
                break
            }
            let decoded = frames[nextIndex] != nil
            guard decoded || broken.contains(nextIndex) else {
                waiting = true
                break
            }
            if decoded {
                reached = nextIndex
            }
            nextDue = (waiting ? now : nextDue) + animation.delays[nextIndex]
            waiting = false
            nextIndex += 1
            if nextIndex == count {
                nextIndex = 0
                loopsDone += 1
            }
            steps += 1
        }
        if let reached {
            show(reached)
        }
        let ended =
            nextIndex == 0 && animation.loopCount > 0 && loopsDone >= animation.loopCount
            && now >= nextDue - 1e-9
        if ended {
            stop()
            return
        }
        guard landed || nextIndex != first else {
            return
        }
        // The window starts at the frame the picture needs next.
        frames = frames.filter { (($0.key - nextIndex + count) % count) < Self.window }
        head.withLock { $0 = nextIndex }
        publish()
    }

    // Shows `index` if it's decoded and not the frame on screen.
    private func show(_ index: Int) {
        guard shownIndex != index, let image = frames[index] else {
            return
        }
        shownIndex = index
        onFrame(image, index)
    }

    // Tells the decoder which frames it needn't decode, and that the window may have moved.
    private func publish() {
        settled.withLock { $0 = broken.union(frames.keys) }
        wake.current?.yield()
    }

    private func startDecoding() {
        stopDecoding()
        let animation = animation
        let pixels = pixels
        let fit = fit
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
                    if let image = animation.frame(index, pixels: pixels, fit: fit) {
                        await self?.decoded(index, image, generation: generation)
                    }
                }
                await self?.decodingEnded()
            } }
            return
        }
        let head = head
        let settled = settled
        let (moved, wake) = AsyncStream.makeStream(of: Void.self, bufferingPolicy: .bufferingNewest(1))
        self.wake.withLock { $0 = wake }
        decoding.withLock { $0 = Task.detached(priority: .userInitiated) { [weak self] in
            // The window's frames in order, then asleep until the picture moves.
            var moves = moved.makeAsyncIterator()
            while !Task.isCancelled {
                let first = head.current
                let have = settled.current
                let missing = (0..<min(Self.window, count)).lazy.map { (first + $0) % count }
                    .first { !have.contains($0) }
                guard let index = missing else {
                    guard await moves.next() != nil else {
                        return
                    }
                    continue
                }
                let image = animation.frame(index, pixels: pixels, fit: fit)
                guard !Task.isCancelled else {
                    return
                }
                await self?.decoded(index, image, generation: generation)
            }
        } }
    }

    private func stopDecoding() {
        decoding.withLock { task in
            task?.cancel()
            task = nil
        }
        wake.withLock { wake in
            wake?.finish()
            wake = nil
        }
    }

    private func decodingEnded() {
        decoding.withLock { $0 = nil }
    }

    private func decoded(_ index: Int, _ image: CGImage?, generation: Int) {
        guard generation == self.generation else {
            return
        }
        guard let image else {
            ImageLog.log("animation: frame \(index) of \(animation.frameCount) didn't decode, passed over")
            broken.insert(index)
            keepsAll ? publish() : advance(clock.now, landed: true)
            return
        }
        frames[index] = image
        if keepsAll {
            publish()
            if isPlaying, index == dueIndex {
                show(index)
            }
        } else {
            advance(clock.now, landed: true)
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
