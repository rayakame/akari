import CoreGraphics
import Foundation
import Testing

@testable import AkariKit

// A frame decoded to cover its box (D130): the covering side exactly the box's, the other
// rounded up, never a pixel over that the layer would resample; EXIF orientation applied
// before the size is taken.
struct ImageDecoderTests {
    let images = TestImages()

    @Test
    func theDecodeCoversTheBoxWithoutOvershooting() throws {
        let source = try ImageDecoder.open(try images.png("wide", width: 640, height: 480))

        let image = try #require(ImageDecoder.decode(source, frame: 0, covering: (64, 64)))

        #expect(image.width == 86)
        #expect(image.height == 64)
        let tall = try ImageDecoder.open(try images.png("tall", width: 300, height: 1000))
        let image2 = try #require(ImageDecoder.decode(tall, frame: 0, covering: (100, 50)))
        #expect(image2.width == 100)
        #expect(image2.height == 334)
    }

    @Test
    func anOrientedFileIsSizedAfterItsRotation() throws {
        let url = try images.jpeg("turned", width: 400, height: 200, orientation: 6)
        let source = try ImageDecoder.open(url)

        let image = try #require(ImageDecoder.decode(source, frame: 0, covering: (64, 64)))

        #expect(image.width == 64)
        #expect(image.height == 128)
    }
}
