import Foundation

public enum OrangeSecretStoreProtocol {
    public static let version = 1
    public static let maxSecretBytes = 16 * 1024
    public static let maxBase64SecretCharacters = ((maxSecretBytes + 2) / 3) * 4

    public enum CredentialKey: String, CaseIterable {
        case accessToken = "orange.access-token"
        case refreshToken = "orange.refresh-token"
        case subscriptionCredential = "orange.subscription-credential"
    }

    public enum Failure: String, Error, Equatable {
        case invalidValue = "secret-invalid-value"
        case unavailable = "secret-store-unavailable"
        case permissionDenied = "secret-store-permission-denied"
        case storageFailure = "secret-store-failure"
    }

    public static func requireVersion(_ supplied: Int) throws {
        if supplied != version {
            throw Failure.unavailable
        }
    }

    public static func parseKey(_ value: String) throws -> CredentialKey {
        guard let key = CredentialKey(rawValue: value) else {
            throw Failure.invalidValue
        }
        return key
    }

    public static func decodeValue(_ encoded: String) throws -> Data {
        guard
            !encoded.isEmpty,
            encoded.utf8.count <= maxBase64SecretCharacters,
            var value = Data(base64Encoded: encoded, options: [])
        else {
            throw Failure.invalidValue
        }
        guard
            !value.isEmpty,
            value.count <= maxSecretBytes,
            value.base64EncodedString() == encoded
        else {
            value.resetBytes(in: 0..<value.count)
            throw Failure.invalidValue
        }
        return value
    }
}
