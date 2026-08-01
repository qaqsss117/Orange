import Foundation
import XCTest

@testable import OrangeSecretStoreCore

final class OrangeSecretStoreProtocolTests: XCTestCase {
    func testAcceptsOnlyTheFixedProtocolVersionAndCredentialKeys() throws {
        try OrangeSecretStoreProtocol.requireVersion(OrangeSecretStoreProtocol.version)
        XCTAssertEqual(
            try OrangeSecretStoreProtocol.parseKey("orange.access-token"),
            .accessToken
        )
        XCTAssertEqual(
            try OrangeSecretStoreProtocol.parseKey("orange.refresh-token"),
            .refreshToken
        )
        XCTAssertEqual(
            try OrangeSecretStoreProtocol.parseKey("orange.subscription-credential"),
            .subscriptionCredential
        )

        assertFailure(.unavailable) {
            try OrangeSecretStoreProtocol.requireVersion(
                OrangeSecretStoreProtocol.version + 1
            )
        }
        assertFailure(.invalidValue) {
            _ = try OrangeSecretStoreProtocol.parseKey("orange.future-secret")
        }
    }

    func testDecodesCanonicalBase64WithinTheFixedSizeLimit() throws {
        let expected = Data("orange-secret".utf8)
        var decoded = try OrangeSecretStoreProtocol.decodeValue(
            expected.base64EncodedString()
        )
        XCTAssertEqual(decoded, expected)
        decoded.resetBytes(in: 0..<decoded.count)
    }

    func testRejectsMalformedOrNonCanonicalBase64() {
        for encoded in ["%%%", "AB==", "\u{5bc6}\u{94a5}"] {
            assertFailure(.invalidValue) {
                _ = try OrangeSecretStoreProtocol.decodeValue(encoded)
            }
        }
    }

    func testRejectsEmptyAndOversizedValuesBeforeTheyCanEscape() {
        assertFailure(.invalidValue) {
            _ = try OrangeSecretStoreProtocol.decodeValue("")
        }

        let oversized = Data(
            repeating: 0x5a,
            count: OrangeSecretStoreProtocol.maxSecretBytes + 1
        )
        assertFailure(.invalidValue) {
            _ = try OrangeSecretStoreProtocol.decodeValue(
                oversized.base64EncodedString()
            )
        }

        assertFailure(.invalidValue) {
            let oversizedCharacterCount =
                OrangeSecretStoreProtocol.maxBase64SecretCharacters + 1
            _ = try OrangeSecretStoreProtocol.decodeValue(
                String(
                    repeating: "A",
                    count: oversizedCharacterCount
                )
            )
        }
    }

    private func assertFailure(
        _ expected: OrangeSecretStoreProtocol.Failure,
        file: StaticString = #filePath,
        line: UInt = #line,
        operation: () throws -> Void
    ) {
        XCTAssertThrowsError(try operation(), file: file, line: line) { error in
            XCTAssertEqual(
                error as? OrangeSecretStoreProtocol.Failure,
                expected,
                file: file,
                line: line
            )
        }
    }
}
