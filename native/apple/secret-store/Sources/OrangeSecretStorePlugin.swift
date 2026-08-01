import Foundation
import OrangeSecretStoreCore
import Security
import Tauri

private typealias SecretKey = OrangeSecretStoreProtocol.CredentialKey
private typealias SecretStoreError = OrangeSecretStoreProtocol.Failure

private struct HandshakeArgs: Decodable {
    let protocolVersion: Int
}

private struct KeyArgs: Decodable {
    let protocolVersion: Int
    let key: String
}

private struct StoreArgs: Decodable {
    let protocolVersion: Int
    let key: String
    let valueBase64: String
}

private struct HandshakeResponse: Encodable {
    let protocolVersion: Int
}

private struct LoadResponse: Encodable {
    let found: Bool
    let valueBase64: String?
}

private struct KeychainSecretStore {
    private static let service = "com.orange.vpn.secret-storage.v1"

    func store(_ key: SecretKey, value: Data) throws {
        let query = baseQuery(key)
        let update: [CFString: Any] = [kSecValueData: value]
        let updateStatus = SecItemUpdate(query as CFDictionary, update as CFDictionary)
        if updateStatus == errSecSuccess {
            return
        }
        guard updateStatus == errSecItemNotFound else {
            throw mapStatus(updateStatus)
        }

        var item = query
        item[kSecValueData] = value
        item[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        try requireSuccess(SecItemAdd(item as CFDictionary, nil))
    }

    func load(_ key: SecretKey) throws -> Data? {
        var query = baseQuery(key)
        query[kSecReturnData] = kCFBooleanTrue
        query[kSecMatchLimit] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound {
            return nil
        }
        try requireSuccess(status)
        guard let value = result as? Data else {
            throw SecretStoreError.storageFailure
        }
        return value
    }

    func delete(_ key: SecretKey) throws {
        let status = SecItemDelete(baseQuery(key) as CFDictionary)
        if status != errSecItemNotFound {
            try requireSuccess(status)
        }
    }

    func logout() throws {
        var firstError: Error?
        for key in SecretKey.allCases {
            do {
                try delete(key)
            } catch {
                if firstError == nil {
                    firstError = error
                }
            }
        }
        if let firstError {
            throw firstError
        }
    }

    private func baseQuery(_ key: SecretKey) -> [CFString: Any] {
        [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: Self.service,
            kSecAttrAccount: key.rawValue,
            kSecAttrSynchronizable: kCFBooleanFalse as Any,
        ]
    }

    private func requireSuccess(_ status: OSStatus) throws {
        if status != errSecSuccess {
            throw mapStatus(status)
        }
    }

    private func mapStatus(_ status: OSStatus) -> SecretStoreError {
        switch status {
        case errSecParam:
            return .invalidValue
        case errSecNotAvailable, errSecInteractionNotAllowed:
            return .unavailable
        case errSecAuthFailed, errSecMissingEntitlement:
            return .permissionDenied
        default:
            return .storageFailure
        }
    }
}

final class OrangeSecretStorePlugin: Plugin {
    private let storage = KeychainSecretStore()

    @objc public func handshake(_ invoke: Invoke) {
        execute(invoke) {
            try OrangeSecretStoreProtocol.requireVersion(
                try invoke.parseArgs(HandshakeArgs.self).protocolVersion
            )
            invoke.resolve(
                HandshakeResponse(
                    protocolVersion: OrangeSecretStoreProtocol.version
                )
            )
        }
    }

    @objc public func store(_ invoke: Invoke) {
        execute(invoke) {
            let args = try invoke.parseArgs(StoreArgs.self)
            try OrangeSecretStoreProtocol.requireVersion(args.protocolVersion)
            var value = try OrangeSecretStoreProtocol.decodeValue(args.valueBase64)
            defer { value.resetBytes(in: 0..<value.count) }
            try storage.store(
                try OrangeSecretStoreProtocol.parseKey(args.key),
                value: value
            )
            invoke.resolve()
        }
    }

    @objc public func load(_ invoke: Invoke) {
        execute(invoke) {
            let args = try invoke.parseArgs(KeyArgs.self)
            try OrangeSecretStoreProtocol.requireVersion(args.protocolVersion)
            guard
                var value = try storage.load(
                    try OrangeSecretStoreProtocol.parseKey(args.key)
                )
            else {
                invoke.resolve(LoadResponse(found: false, valueBase64: nil))
                return
            }
            defer { value.resetBytes(in: 0..<value.count) }
            invoke.resolve(
                LoadResponse(found: true, valueBase64: value.base64EncodedString())
            )
        }
    }

    @objc public func delete(_ invoke: Invoke) {
        execute(invoke) {
            let args = try invoke.parseArgs(KeyArgs.self)
            try OrangeSecretStoreProtocol.requireVersion(args.protocolVersion)
            try storage.delete(try OrangeSecretStoreProtocol.parseKey(args.key))
            invoke.resolve()
        }
    }

    @objc public func logout(_ invoke: Invoke) {
        execute(invoke) {
            try OrangeSecretStoreProtocol.requireVersion(
                try invoke.parseArgs(HandshakeArgs.self).protocolVersion
            )
            try storage.logout()
            invoke.resolve()
        }
    }

    private func execute(_ invoke: Invoke, action: () throws -> Void) {
        do {
            try action()
        } catch let error as SecretStoreError {
            invoke.reject(error.rawValue, code: error.rawValue)
        } catch {
            let stable = SecretStoreError.storageFailure
            invoke.reject(stable.rawValue, code: stable.rawValue)
        }
    }
}

@_cdecl("init_plugin_orange_secret_store")
func initPlugin() -> Plugin {
    OrangeSecretStorePlugin()
}
