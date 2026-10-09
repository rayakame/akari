import Foundation

// The bindings carry akari-core's safe-to-show error texts as `description`.

extension ClientError: CustomStringConvertible, LocalizedError {
    public var errorDescription: String? { description }
}

extension TokenStoreError: CustomStringConvertible, LocalizedError {
    public var errorDescription: String? { description }
}

extension LoginError: CustomStringConvertible, LocalizedError {
    public var errorDescription: String? { description }
}

extension LogoutError: CustomStringConvertible, LocalizedError {
    public var errorDescription: String? { description }
}

extension RequestError: CustomStringConvertible, LocalizedError {
    public var errorDescription: String? { description }
}

extension GatewayError: CustomStringConvertible, LocalizedError {
    public var errorDescription: String? { description }
}
