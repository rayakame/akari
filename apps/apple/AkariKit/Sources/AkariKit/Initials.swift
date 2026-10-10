/// The letters a server or avatar without an image shows, by the rule in
/// `docs/ui/server-list.md`.
public enum Initials {
    /// The first character of each word, other characters kept. Never empty; `limit` caps the
    /// count (avatars use 2).
    public static func of(_ name: String, limit: Int? = nil) -> String {
        var initials = ""
        var inWord = false
        for character in name.replacingOccurrences(of: "'s ", with: " ") {
            if character.isWhitespace {
                inWord = false
            } else if character.isLetter || character.isNumber || character == "_" {
                if !inWord {
                    initials.append(character)
                }
                inWord = true
            } else {
                initials.append(character)
                inWord = false
            }
        }
        if let limit {
            initials = String(initials.prefix(limit))
        }
        return initials.isEmpty ? "?" : initials
    }
}
