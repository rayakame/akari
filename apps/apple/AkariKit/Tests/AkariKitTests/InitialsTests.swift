import Testing

@testable import AkariKit

struct InitialsTests {
    @Test(arguments: [
        ("Rust Programming Language", "RPL"), ("akari", "a"), ("Akari's Lab", "AL"),
        ("🌸 Garden", "🌸G"), ("The 'Best' Server", "T'B'S"), ("Café Crème", "CC"),
        ("مجتمع البرمجة", "ما"), ("akari_lab dev", "ad"), ("", "?"), ("   ", "?"),
        ("one two three four five six seven eight", "ottffsse"),
    ])
    func initialsHandleSymbolsEmojiAndEmptyNames(name: String, initials: String) {
        #expect(Initials.of(name) == initials)
    }

    @Test
    func aLimitCapsTheLetters() {
        #expect(Initials.of("Mira Two Three", limit: 2) == "MT")
        #expect(Initials.of("", limit: 2) == "?")
    }
}
