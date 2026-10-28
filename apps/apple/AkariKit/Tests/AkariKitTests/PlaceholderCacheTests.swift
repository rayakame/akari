import CoreGraphics
import Testing

@testable import AkariKit

// Discord's placeholder: the thumbhash decoded by the core (≤ 32 px), as an image a layer shows
// under the media while it loads.
@MainActor
struct PlaceholderCacheTests {
    // From the core's fixtures: a 1280 × 720 screenshot's thumbhash.
    let wide = "3PcNNYSFeXh/d3eld0iHZoZgVwh2"

    @Test
    func aThumbhashBecomesASmallImageOfItsAspect() throws {
        let cache = PlaceholderCache()

        let image = try #require(cache.image(wide))

        #expect(image.width <= 32 && image.height <= 32)
        #expect(image.width > image.height, "wide like its source")
        #expect(image.alphaInfo == .premultipliedLast || image.alphaInfo == .last)
    }

    @Test
    func theSameThumbhashIsDecodedOnce() throws {
        let cache = PlaceholderCache()

        let first = try #require(cache.image(wide))
        let second = try #require(cache.image(wide))

        #expect(first === second)
    }

    @Test
    func garbageIsNoImage() {
        #expect(PlaceholderCache().image("not base64!") == nil)
        #expect(PlaceholderCache().image("") == nil)
    }

    @Test
    func itKeepsABoundedNumber() throws {
        let cache = PlaceholderCache(limit: 2)
        let a = try #require(cache.image(wide))
        _ = cache.image("1QcSHQRnh493V4dIh4eXh1h4kJUI")
        _ = cache.image("3OcRJYB4d3h/iIeHeEh3eIhw+j2w")

        #expect(cache.count == 2)
        #expect(cache.image(wide) !== a, "the oldest went")
    }
}
