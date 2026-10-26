import AppKit
import CoreImage
import SwiftUI
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

    // The official client's code carries its logo in the middle; Akari's carries Akari's, and a
    // phone still reads it.
    @MainActor @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theCodeWithTheLogoStillDecodes(appearance: NSAppearance.Name) throws {
        // On its white tile, as the login screen shows it.
        let side: CGFloat = 176
        let image = try Rendered(
            ZStack {
                Color.white
                QrCodeView(url: url)
            }
            .frame(width: side, height: side)
            .ignoresSafeArea(),
            appearance: appearance, size: NSSize(width: side, height: side))
        let cgImage = try #require(image.rep.cgImage)
        let detector = try #require(
            CIDetector(ofType: CIDetectorTypeQRCode, context: nil, options: [CIDetectorAccuracy: CIDetectorAccuracyHigh]))

        let codes = detector.features(in: CIImage(cgImage: cgImage))

        #expect((codes.first as? CIQRCodeFeature)?.messageString == url)
        // Left of the logo, inside its tile: the brand color.
        let tile = try #require(image.color(side / 2 - QrCodeView.logo * 0.33, side / 2))
        #expect(tile.blueComponent - tile.redComponent > 0.3, "\(tile)")
    }

    @Test
    func anEmptyAddressGivesNoImage() {
        #expect(QrCode.image(for: "", side: 176) == nil)
    }
}
