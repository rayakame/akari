import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct DirectMessageListTests {
    @Test
    func memberLinesAreSingularForOne() {
        #expect(DirectMessageList.memberLine(1) == "1 Member")
        #expect(DirectMessageList.memberLine(4) == "4 Members")
    }

    struct Ink {
        var rows = 0
        var minX = Int.max
        var maxX = Int.min
        var brightest: CGFloat = 0

        func width(_ image: Rendered) -> CGFloat {
            CGFloat(maxX - minX + 1) / image.scale
        }
    }

    // The first run of pixel rows from `top` down that differ from `background` within `xs`.
    func firstInk(_ image: Rendered, xs: Range<CGFloat>, below top: Int, background: NSColor?)
        -> Ink
    {
        let columns = Int(xs.lowerBound * image.scale)..<Int(xs.upperBound * image.scale)
        var ink = Ink()
        for y in top..<image.rep.pixelsHigh {
            let marked = columns.filter { !same(image.pixel($0, y), background) }
            guard let first = marked.first, let last = marked.last else {
                if ink.rows > 0 {
                    break
                }
                continue
            }
            ink.rows += 1
            ink.minX = min(ink.minX, first)
            ink.maxX = max(ink.maxX, last)
            for x in marked {
                if let pixel = image.pixel(x, y) {
                    ink.brightest = max(
                        ink.brightest, pixel.redComponent, pixel.greenComponent,
                        pixel.blueComponent)
                }
            }
        }
        return ink
    }

    func inkWidth(_ style: TextStyle) throws -> CGFloat {
        let image = try Rendered(
            Text("Direct Messages")
                .font(style.font)
                .foregroundStyle(Color(nsColor: Palette.channelDefault))
                .fixedSize()
                .frame(width: 300, height: 40, alignment: .leading)
                .background(Color(nsColor: Palette.frame)),
            size: NSSize(width: 300, height: 40))
        return firstInk(image, xs: 0..<300, below: 0, background: image.color(299, 1))
            .width(image)
    }

    // As in the official client: small and gray, like a category, below the line that runs on
    // from the channel header.
    @Test
    func theDirectMessagesLabelIsSetLikeACategory() throws {
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: CrowdedAccount(),
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(channel: CrowdedStore.dmIds[0])
        let image = try Rendered(SessionView(session: session) { _, _ in Color.clear })
        let sidebar = SidebarWidth.stored(in: .standard)
        let listX = sidebar - 30
        let line = try #require(image.firstChange(at: listX, below: TitleBar.height + 24))

        // Clear of the list's left border and of a scroll bar.
        let label = firstInk(
            image, xs: (SidebarWidth.rail + 2)..<(sidebar - 20), below: line + Int(2 * image.scale),
            background: image.color(listX, TitleBar.height + 24))
        let width = label.width(image)
        let category = try inkWidth(TypeScale.category)

        #expect(abs(width - category) <= 1, "\(width) pt wide, a category \(category)")
        #expect(try inkWidth(TypeScale.serverHeader) - category > 4)
        #expect(label.brightest < 0.7, "gray, not white: \(label.brightest)")
        #expect(
            abs(CGFloat(label.minX) / image.scale - (SidebarWidth.rail + 1 + 16)) <= 2,
            "starts at \(CGFloat(label.minX) / image.scale)")
    }
}
