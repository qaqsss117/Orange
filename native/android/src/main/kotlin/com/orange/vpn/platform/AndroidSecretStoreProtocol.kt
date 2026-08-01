package com.orange.vpn.platform

internal object AndroidSecretStoreProtocol {
    const val VERSION = 1
    const val MAX_SECRET_BYTES = 16 * 1024
    const val MAX_BASE64_SECRET_CHARS = ((MAX_SECRET_BYTES + 2) / 3) * 4

    fun requireVersion(supplied: Int) {
        if (supplied != VERSION) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.Unavailable)
        }
    }

    fun parseKey(value: String): AndroidSecretKey =
        AndroidSecretKey.entries.firstOrNull { it.storageName == value }
            ?: throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)

    fun decodeValue(
        encoded: String,
        decode: (String) -> ByteArray,
        encode: (ByteArray) -> ByteArray
    ): ByteArray {
        if (
            encoded.isEmpty() ||
            encoded.length > MAX_BASE64_SECRET_CHARS ||
            encoded.any { it.code > 0x7f }
        ) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)
        }

        val decoded =
            try {
                decode(encoded)
            } catch (_: IllegalArgumentException) {
                throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)
            }
        val supplied = encoded.toByteArray(Charsets.US_ASCII)
        val canonical =
            try {
                encode(decoded)
            } catch (_: IllegalArgumentException) {
                decoded.fill(0)
                supplied.fill(0)
                throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)
            }
        try {
            if (
                decoded.isEmpty() ||
                decoded.size > MAX_SECRET_BYTES ||
                !supplied.contentEquals(canonical)
            ) {
                decoded.fill(0)
                throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)
            }
        } finally {
            supplied.fill(0)
            canonical.fill(0)
        }
        return decoded
    }
}
