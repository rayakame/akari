import AppKit

extension NSColor {
    /// `#rrggbb` in sRGB.
    convenience init(hex: String, alpha: CGFloat = 1) {
        let value = UInt32(hex.dropFirst(), radix: 16) ?? 0
        let channel = { (shift: UInt32) in CGFloat((value >> shift) & 0xff) / 255 }
        self.init(srgbRed: channel(16), green: channel(8), blue: channel(0), alpha: alpha)
    }
}
