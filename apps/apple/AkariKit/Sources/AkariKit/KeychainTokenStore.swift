import Foundation
import Security

/// Tokens in the Keychain: generic passwords, one per account. Never logs.
public final class KeychainTokenStore: TokenStore {
    private let service: String

    public init(service: String = "app.akari.tokens") {
        self.service = service
    }

    public func load(account: UserId) throws -> String? {
        var query = item(account)
        query[kSecReturnData] = true
        query[kSecMatchLimit] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound {
            return nil
        }
        try check(status)
        guard let data = result as? Data, let token = String(data: data, encoding: .utf8) else {
            throw TokenStoreError.Backend(message: "the stored token isn't text")
        }
        return token
    }

    public func save(account: UserId, token: String) throws {
        let data = Data(token.utf8)
        let status = SecItemUpdate(
            item(account) as CFDictionary,
            [kSecValueData: data] as CFDictionary
        )
        if status != errSecItemNotFound {
            return try check(status)
        }
        var add = item(account)
        add[kSecValueData] = data
        try check(SecItemAdd(add as CFDictionary, nil))
    }

    public func delete(account: UserId) throws {
        let status = SecItemDelete(item(account) as CFDictionary)
        if status != errSecItemNotFound {
            try check(status)
        }
    }

    private func item(_ account: UserId) -> [CFString: Any] {
        [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account.description,
            // Tokens stay on this Mac, never in iCloud Keychain.
            kSecAttrSynchronizable: false,
        ]
    }
}

private func check(_ status: OSStatus) throws {
    switch status {
    case errSecSuccess:
        return
    case errSecInteractionNotAllowed:
        throw TokenStoreError.Unavailable
    default:
        let text = SecCopyErrorMessageString(status, nil) as String? ?? "unknown error"
        throw TokenStoreError.Backend(message: "Keychain error \(status): \(text)")
    }
}
