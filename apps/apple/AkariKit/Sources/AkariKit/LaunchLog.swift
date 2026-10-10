import Darwin
import Foundation
import OSLog
import os

/// Launch milestones, each logged once per process with its time since the process started.
/// The log holds the milestone's name and a duration, nothing else.
public enum LaunchLog {
    private static let logger = Logger(subsystem: "app.akari", category: "launch")
    private static let marked = OSAllocatedUnfairLock<Set<String>>(initialState: [])

    /// Logs `name` the first time; returns whether it did.
    @discardableResult
    public static func mark(_ name: String) -> Bool {
        let first = marked.withLock { $0.insert(name).inserted }
        guard first else {
            return false
        }
        let milliseconds = sinceProcessStart().map { Int($0 * 1000) } ?? -1
        logger.info("\(name, privacy: .public) after \(milliseconds, privacy: .public) ms")
        return true
    }

    // From the kernel, so dyld and Rust start-up count too.
    static func sinceProcessStart() -> TimeInterval? {
        var info = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.stride
        var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
        guard sysctl(&name, u_int(name.count), &info, &size, nil, 0) == 0 else {
            return nil
        }
        let start = info.kp_proc.p_un.__p_starttime
        let started = TimeInterval(start.tv_sec) + TimeInterval(start.tv_usec) / 1e6
        return Date().timeIntervalSince1970 - started
    }
}
