import Foundation
import Testing

@testable import Akari

struct SidebarWidthTests {
    @Test
    func theWidthIsClampedAndRemembered() {
        let defaults = MemoryDefaults()

        #expect(SidebarWidth.stored(in: defaults) == 375)
        #expect(SidebarWidth.clamped(200) == 264)
        #expect(SidebarWidth.clamped(500) == 432)
        SidebarWidth.store(300, in: defaults)
        #expect(SidebarWidth.stored(in: defaults) == 300)
        SidebarWidth.store(9000, in: defaults)
        #expect(SidebarWidth.stored(in: defaults) == 432)
        #expect(SidebarWidth.listWidth(for: 375) == 302)
    }
}
