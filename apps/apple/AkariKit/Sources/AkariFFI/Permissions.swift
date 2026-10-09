/// A permission bitfield, with the bits akari-core uses.
public struct Permissions: OptionSet, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }

    public static let all = Permissions(rawValue: .max)
    public static let administrator = Permissions(rawValue: 1 << 3)
    public static let viewChannel = Permissions(rawValue: 1 << 10)
    public static let sendMessages = Permissions(rawValue: 1 << 11)
    public static let sendTtsMessages = Permissions(rawValue: 1 << 12)
    public static let embedLinks = Permissions(rawValue: 1 << 14)
    public static let attachFiles = Permissions(rawValue: 1 << 15)
    public static let readMessageHistory = Permissions(rawValue: 1 << 16)
    public static let mentionEveryone = Permissions(rawValue: 1 << 17)
    public static let changeNickname = Permissions(rawValue: 1 << 26)
    public static let sendMessagesInThreads = Permissions(rawValue: 1 << 38)
}
