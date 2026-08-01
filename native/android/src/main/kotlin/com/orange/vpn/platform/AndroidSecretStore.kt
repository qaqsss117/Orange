package com.orange.vpn.platform

import android.content.Context
import android.content.SharedPreferences
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.KeyStoreException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

internal enum class AndroidSecretKey(val storageName: String) {
    AccessToken("orange.access-token"),
    RefreshToken("orange.refresh-token"),
    SubscriptionCredential("orange.subscription-credential"),
}

internal enum class AndroidSecretStoreError(val code: String) {
    InvalidValue("secret-invalid-value"),
    Unavailable("secret-store-unavailable"),
    PermissionDenied("secret-store-permission-denied"),
    StorageFailure("secret-store-failure"),
}

internal class AndroidSecretStoreException(val error: AndroidSecretStoreError) :
    Exception(error.code)

internal class AndroidSecretStore(context: Context) {
    private val preferences: SharedPreferences =
        context.applicationContext.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    fun store(key: AndroidSecretKey, value: ByteArray) {
        try {
            synchronized(LOCK) {
                if (
                    value.isEmpty() ||
                        value.size > AndroidSecretStoreProtocol.MAX_SECRET_BYTES
                ) {
                    throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)
                }
                stable {
                    val cipher = Cipher.getInstance(CIPHER_TRANSFORMATION)
                    cipher.init(Cipher.ENCRYPT_MODE, getOrCreateKey())
                    cipher.updateAAD(key.storageName.toByteArray(Charsets.UTF_8))
                    val initializationVector = cipher.iv.copyOf()
                    val ciphertext = cipher.doFinal(value)
                    try {
                        val payload = encodePayload(initializationVector, ciphertext)
                        try {
                            val encoded = Base64.encodeToString(payload, Base64.NO_WRAP)
                            if (!preferences.edit().putString(key.storageName, encoded).commit()) {
                                throw AndroidSecretStoreException(
                                    AndroidSecretStoreError.StorageFailure,
                                )
                            }
                        } finally {
                            payload.fill(0)
                        }
                    } finally {
                        initializationVector.fill(0)
                        ciphertext.fill(0)
                    }
                }
            }
        } finally {
            value.fill(0)
        }
    }

    fun load(key: AndroidSecretKey): ByteArray? =
        synchronized(LOCK) {
            stable {
                val encoded = preferences.getString(key.storageName, null) ?: return@stable null
                val payload = Base64.decode(encoded, Base64.NO_WRAP)
                try {
                    decodePayload(key, payload)
                } finally {
                    payload.fill(0)
                }
            }
        }

    fun delete(key: AndroidSecretKey) {
        synchronized(LOCK) {
            stable {
                if (!preferences.edit().remove(key.storageName).commit()) {
                    throw AndroidSecretStoreException(AndroidSecretStoreError.StorageFailure)
                }
            }
        }
    }

    fun logout() {
        synchronized(LOCK) {
            var firstError: AndroidSecretStoreException? = null
            for (key in USER_SECRET_KEYS) {
                try {
                    delete(key)
                } catch (error: AndroidSecretStoreException) {
                    if (firstError == null) {
                        firstError = error
                    }
                }
            }
            try {
                deleteEncryptionKey()
            } catch (error: AndroidSecretStoreException) {
                if (firstError == null) {
                    firstError = error
                }
            }
            firstError?.let { throw it }
        }
    }

    private fun decodePayload(key: AndroidSecretKey, payload: ByteArray): ByteArray {
        if (
            payload.size < HEADER_BYTES + GCM_IV_BYTES + GCM_TAG_BYTES ||
                payload[0] != FORMAT_VERSION ||
                (payload[1].toInt() and 0xff) != GCM_IV_BYTES
        ) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.StorageFailure)
        }

        val ciphertextOffset = HEADER_BYTES + GCM_IV_BYTES
        val initializationVector = payload.copyOfRange(HEADER_BYTES, ciphertextOffset)
        val ciphertext = payload.copyOfRange(ciphertextOffset, payload.size)
        try {
            val cipher = Cipher.getInstance(CIPHER_TRANSFORMATION)
            cipher.init(
                Cipher.DECRYPT_MODE,
                getExistingKey(),
                GCMParameterSpec(GCM_TAG_BITS, initializationVector),
            )
            cipher.updateAAD(key.storageName.toByteArray(Charsets.UTF_8))
            val value = cipher.doFinal(ciphertext)
            if (
                value.isEmpty() ||
                    value.size > AndroidSecretStoreProtocol.MAX_SECRET_BYTES
            ) {
                value.fill(0)
                throw AndroidSecretStoreException(AndroidSecretStoreError.StorageFailure)
            }
            return value
        } finally {
            initializationVector.fill(0)
            ciphertext.fill(0)
        }
    }

    private fun getOrCreateKey(): SecretKey {
        val keyStore = openKeyStore()
        loadKey(keyStore)?.let { return it }

        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE_PROVIDER)
        generator.init(
            KeyGenParameterSpec.Builder(
                    KEY_ALIAS,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .setUserAuthenticationRequired(false)
                .build(),
        )
        return generator.generateKey()
    }

    private fun getExistingKey(): SecretKey =
        loadKey(openKeyStore())
            ?: throw AndroidSecretStoreException(AndroidSecretStoreError.Unavailable)

    private fun deleteEncryptionKey() {
        stable {
            val keyStore = openKeyStore()
            if (keyStore.containsAlias(KEY_ALIAS)) {
                keyStore.deleteEntry(KEY_ALIAS)
            }
        }
    }

    private fun openKeyStore(): KeyStore =
        KeyStore.getInstance(KEYSTORE_PROVIDER).apply { load(null) }

    private fun loadKey(keyStore: KeyStore): SecretKey? {
        val key = keyStore.getKey(KEY_ALIAS, null) ?: return null
        return key as? SecretKey
            ?: throw AndroidSecretStoreException(AndroidSecretStoreError.StorageFailure)
    }

    private fun encodePayload(initializationVector: ByteArray, ciphertext: ByteArray): ByteArray {
        if (initializationVector.size != GCM_IV_BYTES) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.StorageFailure)
        }
        return ByteArray(HEADER_BYTES + initializationVector.size + ciphertext.size).also { payload ->
            payload[0] = FORMAT_VERSION
            payload[1] = initializationVector.size.toByte()
            initializationVector.copyInto(payload, HEADER_BYTES)
            ciphertext.copyInto(payload, HEADER_BYTES + initializationVector.size)
        }
    }

    private inline fun <T> stable(operation: () -> T): T {
        try {
            return operation()
        } catch (error: AndroidSecretStoreException) {
            throw error
        } catch (_: SecurityException) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.PermissionDenied)
        } catch (_: KeyStoreException) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.Unavailable)
        } catch (_: IOException) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.Unavailable)
        } catch (_: GeneralSecurityException) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.StorageFailure)
        } catch (_: RuntimeException) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.StorageFailure)
        }
    }

    private companion object {
        const val KEYSTORE_PROVIDER = "AndroidKeyStore"
        const val KEY_ALIAS = "com.orange.vpn.secret-storage.v1"
        const val PREFERENCES_NAME = "orange.secure-secrets.v1"
        const val CIPHER_TRANSFORMATION = "AES/GCM/NoPadding"
        const val HEADER_BYTES = 2
        const val GCM_IV_BYTES = 12
        const val GCM_TAG_BYTES = 16
        const val GCM_TAG_BITS = GCM_TAG_BYTES * 8
        const val FORMAT_VERSION: Byte = 1
        val USER_SECRET_KEYS =
            arrayOf(
                AndroidSecretKey.AccessToken,
                AndroidSecretKey.RefreshToken,
                AndroidSecretKey.SubscriptionCredential,
            )
        val LOCK = Any()
    }
}
