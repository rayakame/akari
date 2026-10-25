import CoreGraphics
import Foundation
import ImageIO
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

    // The corrective redraw takes a grayscale or CMYK source too (ImageIO's thumbnails of a gray
    // file stay gray; a CMYK file's come back RGB).
    @Test
    func grayscaleAndCmykSourcesAreRedrawnToo() throws {
        let source = try ImageDecoder.open(try images.grayPNG("gray", width: 640, height: 480))
        #expect(CGImageSourceCreateImageAtIndex(source, 0, nil)?.colorSpace?.model == .monochrome)

        let image = try #require(ImageDecoder.decode(source, frame: 0, covering: (64, 64)))

        #expect(image.width == 86)
        #expect(image.height == 64)

        let cmyk = try ImageDecoder.open(try images.cmykJPEG("cmyk", width: 640, height: 480))
        let printed = try #require(ImageDecoder.decode(cmyk, frame: 0, covering: (64, 64)))
        #expect(printed.width == 86)
        #expect(printed.height == 64)
    }

    // Emoji and stickers fit inside their box (the smaller ratio), never cut, never upscaled.
    @Test
    func aContainedDecodeFitsInsideTheBox() throws {
        let wide = try ImageDecoder.open(try images.png("wide2", width: 640, height: 480))
        let fitted = try #require(ImageDecoder.decode(wide, frame: 0, fitting: (64, 64)))
        #expect(fitted.width == 64)
        #expect(fitted.height == 48)

        let tall = try ImageDecoder.open(try images.png("tall2", width: 300, height: 1000))
        let fittedTall = try #require(ImageDecoder.decode(tall, frame: 0, fitting: (100, 50)))
        #expect(fittedTall.width == 15)
        #expect(fittedTall.height == 50)

        let small = try ImageDecoder.open(try images.png("small", width: 20, height: 10))
        let kept = try #require(ImageDecoder.decode(small, frame: 0, fitting: (64, 64)))
        #expect(kept.width == 20 && kept.height == 10, "never above the source")
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
