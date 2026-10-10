import Testing

@testable import Akari

struct DirectMessageListTests {
    @Test
    func memberLinesAreSingularForOne() {
        #expect(DirectMessageList.memberLine(1) == "1 Member")
        #expect(DirectMessageList.memberLine(4) == "4 Members")
    }
}
