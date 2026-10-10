import Foundation

extension HostInfo {
    /// This Mac: the kernel release and the user's BCP 47 locale.
    public static var current: HostInfo {
        HostInfo(osVersion: kernelRelease(), systemLocale: Locale.current.identifier(.bcp47))
    }
}

private func kernelRelease() -> String {
    var name = utsname()
    guard uname(&name) == 0 else {
        return ""
    }
    return withUnsafeBytes(of: &name.release) { bytes in
        String(decoding: bytes.prefix { $0 != 0 }, as: UTF8.self)
    }
}
