import AppKit
import CoreImage
import CoreImage.CIFilterBuiltins

/// QR codes for the login screen, drawn by the app.
enum QrCode {
    /// The modules per side, the generator's margin included.
    static func moduleCount(for text: String) -> Int? {
        code(for: text).map { Int($0.extent.width) }
    }

    /// Black on white, scaled by a whole factor so every module stays sharp; at most `side`.
    static func image(for text: String, side: CGFloat) -> NSImage? {
        guard let code = code(for: text) else {
            return nil
        }
        let scale = max(1, (side / code.extent.width).rounded(.down))
        let scaled = code.samplingNearest().transformed(
            by: CGAffineTransform(scaleX: scale, y: scale))
        guard let image = CIContext().createCGImage(scaled, from: scaled.extent) else {
            return nil
        }
        return NSImage(cgImage: image, size: NSSize(width: image.width, height: image.height))
    }

    private static func code(for text: String) -> CIImage? {
        guard !text.isEmpty else {
            return nil
        }
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        return filter.outputImage
    }
}
