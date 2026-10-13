import Testing

@testable import AkariKit

struct MentionTextTests {
    @Test
    func mentionTextsForEveryName() {
        let known = MentionName.known(name: "mira")
        #expect(MentionText.user(known) == "@mira")
        #expect(MentionText.user(.missing) == nil)
        #expect(MentionText.user(.notLoaded) == "@user")
        #expect(MentionText.role(.known(name: "Moderators")) == "@Moderators")
        #expect(MentionText.role(.missing) == "@unknown-role")
        #expect(MentionText.role(.notLoaded) == "@role")
        #expect(MentionText.channel(.known(name: "general")) == "general")
        #expect(MentionText.channel(.missing) == "unknown")
        #expect(MentionText.channel(.noAccess) == "No Access")
        #expect(MentionText.channel(.notLoaded) == "channel")
        #expect(MentionText.command("role add") == "/role add")
        #expect(MentionText.navigation(.customize) == "Channels & Roles")
        #expect(MentionText.navigation(.browse) == "Browse Channels")
        #expect(MentionText.navigation(.guide) == "Server Guide")
        #expect(MentionText.navigation(.linkedRoles(roleId: nil)) == "Linked Roles")
    }

    @Test
    func notLoadedNamesArePlaceholders() {
        #expect(MentionText.isPlaceholder(.notLoaded))
        #expect(!MentionText.isPlaceholder(.known(name: "a")))
        #expect(!MentionText.isPlaceholder(.missing))
        #expect(!MentionText.isPlaceholder(.noAccess))
    }
}
