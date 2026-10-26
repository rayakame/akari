import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct TitleBarTests {
    @Test
    func theTitleNamesWhereTheUserIs() {
        let server = GuildId(rawValue: 1)

        #expect(
            WindowTitle.at(.guild(server), server: "testserver", conversation: nil)
                == WindowTitle(text: "testserver", icon: .server("testserver")))
        #expect(
            WindowTitle.at(.home, server: nil, conversation: nil)
                == WindowTitle(text: "Direct Messages", icon: .home))
        #expect(
            WindowTitle.at(.home, server: nil, conversation: (name: "Mira", avatar: .avatar(7)))
                == WindowTitle(text: "Mira", icon: .avatar(.avatar(7))))
        // Before READY the store doesn't know the server yet.
        #expect(WindowTitle.at(.guild(server), server: nil, conversation: nil).text == "Akari")
    }

    // Measured in the official client.
    @Test
    func theTitleAndTheCornerUseTheMeasuredValues() {
        #expect(TitleBar.height == 32)
        #expect(TitleBar.iconSize == 16)
        #expect(TitleBar.iconRadius == 4)
        #expect(TitleBar.gap == 8)
        #expect(TypeScale.titleBar == TextStyle(12.5, .medium))
        #expect(ContentCorner.radius == 11.5)
    }

    func session(atHome: Bool) -> SessionModel {
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: CrowdedAccount(),
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        if atHome {
            session.open(channel: CrowdedStore.dmIds[0])
        } else {
            session.open(.guild(CrowdedStore.guildIds[0]))
        }
        return session
    }

    // The ink in the title bar, in points: the official client centers it on the window, not on
    // the area right of the sidebar.
    @Test(arguments: [false, true])
    func theTitleIsCenteredOnTheWindow(atHome: Bool) throws {
        let image = try Rendered(
            SessionView(session: session(atHome: atHome)) { _, _ in Color.clear })
        let background = try #require(image.color(4, 4))
        let columns = (0..<image.rep.pixelsWide).filter { x in
            (0..<Int(TitleBar.height * image.scale)).contains { y in
                !same(image.pixel(x, y), background)
            }
        }
        let first = try #require(columns.first)
        let last = try #require(columns.last)

        let middle = CGFloat(first + last + 1) / 2 / image.scale
        #expect(abs(middle - image.width / 2) <= 1, "the title's middle is at \(middle)")
        #expect(CGFloat(last - first) / image.scale > TitleBar.iconSize + TitleBar.gap)
    }

    // The rail and the title bar form one frame; the area right of them starts with a rounded
    // corner and a 1 pt line along its top and left edges.
    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theContentSitsInTheFrameWithARoundedCorner(appearance: NSAppearance.Name) throws {
        let image = try Rendered(
            SessionView(session: session(atHome: false)) { _, _ in Color.clear },
            appearance: appearance)
        let frame = try #require(image.color(4, 4))
        let rail = SidebarWidth.rail
        let top = TitleBar.height
        let line = try #require(image.color(rail + 0.5, top + 40))

        #expect(!same(line, frame), "the left edge has its line")
        #expect(same(image.color(rail + 40, top + 0.5), line), "the top edge over the list")
        #expect(same(image.color(image.width - 20, top + 0.5), line), "the top edge over the page")
        #expect(same(image.color(rail + 0.5, top + 0.5), frame), "the corner is rounded off")
        #expect(same(image.color(6, top + 40), frame), "the rail is the frame's color")
        #expect(same(image.color(rail + 40, top / 2), frame), "and so is the title bar")
    }
}
