import AkariKit
import Foundation
import Testing

struct HostInfoTests {
    @Test
    func hostInfoHasAKernelReleaseAndABcp47Locale() {
        let host = HostInfo.current

        #expect(host.osVersion.wholeMatch(of: /\d+\.\d+(\.\d+)?/) != nil)
        #expect(host.osVersion == ProcessInfo.processInfo.kernelRelease)
        #expect(!host.systemLocale.isEmpty)
        #expect(!host.systemLocale.contains("_"))
    }
}

extension ProcessInfo {
    // `sysctl kern.osrelease` is what `uname -r` prints.
    fileprivate var kernelRelease: String? {
        var size = 0
        guard sysctlbyname("kern.osrelease", nil, &size, nil, 0) == 0 else { return nil }
        var buffer = [CChar](repeating: 0, count: size)
        guard sysctlbyname("kern.osrelease", &buffer, &size, nil, 0) == 0 else { return nil }
        return String(decoding: buffer.prefix { $0 != 0 }.map(UInt8.init(bitPattern:)), as: UTF8.self)
    }
}
