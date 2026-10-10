import Darwin
import Foundation
import OSLog

/// Logs once per process how long it took from launch until a message list showed rows, for
/// the launch measurement in the PR. Only the duration is logged.
enum FirstChannelTimer {
    private static var reported = false

    static func tableShowedRows() {
        guard !reported, let start = processStart else {
            return
        }
        reported = true
        let milliseconds = Int(Date().timeIntervalSince(start) * 1000)
        Logger(subsystem: "app.akari", category: "launch")
            .info("first channel shown after \(milliseconds, privacy: .public) ms")
    }

    // From the kernel, so dyld and Rust start-up count too.
    private static var processStart: Date? {
        var info = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.stride
        var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
        guard sysctl(&name, u_int(name.count), &info, &size, nil, 0) == 0 else {
            return nil
        }
        let start = info.kp_proc.p_un.__p_starttime
        return Date(
            timeIntervalSince1970: TimeInterval(start.tv_sec) + TimeInterval(start.tv_usec) / 1e6)
    }
}
