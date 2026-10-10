import AppKit
import CoreImage
import Testing

@testable import Akari

struct QrCodeTests {
    let url = "https://discord.com/ra/2D0SBG6LE4_wZvtDsJb7v1HM5mXeMyE3gwrbZv2plgs"

    @Test
    func aCodeDecodesBackToItsUrl() throws {
        let image = try #require(QrCode.image(for: url, side: 176))
        let cgImage = try #require(image.cgImage(forProposedRect: nil, context: nil, hints: nil))
        let detector = try #require(
            CIDetector(ofType: CIDetectorTypeQRCode, context: nil, options: nil))

        let codes = detector.features(in: CIImage(cgImage: cgImage))

        #expect((codes.first as? CIQRCodeFeature)?.messageString == url)
    }

    @Test
    func theImageIsSharpAtTheRequestedSide() throws {
        let image = try #require(QrCode.image(for: url, side: 176))
        let modules = try #require(QrCode.moduleCount(for: url))
        let side = Int(image.size.width)

        #expect(image.size.width == image.size.height)
        #expect(side <= 176)
        #expect(side % modules == 0)
        #expect(side / modules >= 2)
    }

    @Test
    func anEmptyAddressGivesNoImage() {
        #expect(QrCode.image(for: "", side: 176) == nil)
    }
}
