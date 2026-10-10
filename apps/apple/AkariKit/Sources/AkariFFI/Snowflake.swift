/// A Discord ID, typed by what it identifies, like `Snowflake<M>` in akari-core.
public struct Snowflake<Marker>: RawRepresentable, Hashable, Comparable, Sendable,
    CustomStringConvertible
{
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }

    public static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.rawValue < rhs.rawValue
    }

    public var description: String {
        String(rawValue)
    }
}

public enum UserMarker {}
public enum GuildMarker {}
public enum ChannelMarker {}
public enum MessageMarker {}
public enum AttachmentMarker {}

public typealias UserId = Snowflake<UserMarker>
public typealias GuildId = Snowflake<GuildMarker>
public typealias ChannelId = Snowflake<ChannelMarker>
public typealias MessageId = Snowflake<MessageMarker>
public typealias AttachmentId = Snowflake<AttachmentMarker>
