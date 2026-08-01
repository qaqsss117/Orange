package com.orange.vpn.platform

import java.util.Base64
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidSecretStoreProtocolTest {
    @Test
    fun acceptsOnlyTheFixedProtocolVersionAndCredentialKeys() {
        AndroidSecretStoreProtocol.requireVersion(AndroidSecretStoreProtocol.VERSION)
        assertSame(
            AndroidSecretKey.AccessToken,
            AndroidSecretStoreProtocol.parseKey("orange.access-token")
        )
        assertSame(
            AndroidSecretKey.RefreshToken,
            AndroidSecretStoreProtocol.parseKey("orange.refresh-token")
        )
        assertSame(
            AndroidSecretKey.SubscriptionCredential,
            AndroidSecretStoreProtocol.parseKey("orange.subscription-credential")
        )

        assertStableError(AndroidSecretStoreError.Unavailable) {
            AndroidSecretStoreProtocol.requireVersion(AndroidSecretStoreProtocol.VERSION + 1)
        }
        assertStableError(AndroidSecretStoreError.InvalidValue) {
            AndroidSecretStoreProtocol.parseKey("orange.future-secret")
        }
    }

    @Test
    fun decodesCanonicalBase64WithinTheFixedSizeLimit() {
        val expected = "orange-secret".toByteArray()
        val encoded = Base64.getEncoder().encodeToString(expected)
        val decoded =
            AndroidSecretStoreProtocol.decodeValue(
                encoded,
                Base64.getDecoder()::decode,
                Base64.getEncoder()::encode
            )

        assertArrayEquals(expected, decoded)
        decoded.fill(0)
    }

    @Test
    fun rejectsMalformedOrNonCanonicalBase64() {
        assertStableError(AndroidSecretStoreError.InvalidValue) {
            AndroidSecretStoreProtocol.decodeValue(
                "%%%",
                Base64.getDecoder()::decode,
                Base64.getEncoder()::encode
            )
        }

        val nonCanonicalDecoded = byteArrayOf(0)
        assertStableError(AndroidSecretStoreError.InvalidValue) {
            AndroidSecretStoreProtocol.decodeValue(
                "AB==",
                { nonCanonicalDecoded },
                { Base64.getEncoder().encode(it) }
            )
        }
        assertTrue(nonCanonicalDecoded.all { it == 0.toByte() })

        assertStableError(AndroidSecretStoreError.InvalidValue) {
            AndroidSecretStoreProtocol.decodeValue(
                "\u5bc6\u94a5",
                Base64.getDecoder()::decode,
                Base64.getEncoder()::encode
            )
        }
    }

    @Test
    fun rejectsEmptyAndOversizedValuesBeforeTheyCanEscape() {
        var decoderCalled = false
        assertStableError(AndroidSecretStoreError.InvalidValue) {
            AndroidSecretStoreProtocol.decodeValue(
                "",
                {
                    decoderCalled = true
                    byteArrayOf(1)
                },
                Base64.getEncoder()::encode
            )
        }
        assertFalse(decoderCalled)

        val oversized = ByteArray(AndroidSecretStoreProtocol.MAX_SECRET_BYTES + 1) { 0x5a }
        assertStableError(AndroidSecretStoreError.InvalidValue) {
            AndroidSecretStoreProtocol.decodeValue(
                "Wg==",
                { oversized },
                Base64.getEncoder()::encode
            )
        }
        assertTrue(oversized.all { it == 0.toByte() })

        assertStableError(AndroidSecretStoreError.InvalidValue) {
            AndroidSecretStoreProtocol.decodeValue(
                "A".repeat(AndroidSecretStoreProtocol.MAX_BASE64_SECRET_CHARS + 1),
                {
                    decoderCalled = true
                    byteArrayOf(1)
                },
                Base64.getEncoder()::encode
            )
        }
        assertFalse(decoderCalled)
    }

    private fun assertStableError(expected: AndroidSecretStoreError, operation: () -> Unit) {
        val error =
            assertThrows(AndroidSecretStoreException::class.java) {
                operation()
            }
        assertEquals(expected, error.error)
        assertEquals(expected.code, error.message)
    }
}
