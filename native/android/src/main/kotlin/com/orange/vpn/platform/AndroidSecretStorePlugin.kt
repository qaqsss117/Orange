package com.orange.vpn.platform

import android.app.Activity
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class SecretStoreHandshakeArgs {
    var protocolVersion: Int = 0
}

@InvokeArg
class SecretStoreKeyArgs {
    var protocolVersion: Int = 0
    lateinit var key: String
}

@InvokeArg
class StoreSecretArgs {
    var protocolVersion: Int = 0
    lateinit var key: String
    lateinit var valueBase64: String
}

@TauriPlugin
class AndroidSecretStorePlugin(private val activity: Activity) : Plugin(activity) {
    private val storage = AndroidSecretStore(activity)

    @Command
    fun handshake(invoke: Invoke) {
        execute(invoke) {
            requireProtocol(invoke.parseArgs(SecretStoreHandshakeArgs::class.java).protocolVersion)
            val response = JSObject().put("protocolVersion", PROTOCOL_VERSION)
            invoke.resolve(response)
        }
    }

    @Command
    fun store(invoke: Invoke) {
        execute(invoke) {
            val args = invoke.parseArgs(StoreSecretArgs::class.java)
            requireProtocol(args.protocolVersion)
            val value = decodeValue(args.valueBase64)
            try {
                storage.store(parseKey(args.key), value)
            } finally {
                value.fill(0)
            }
            invoke.resolve()
        }
    }

    @Command
    fun load(invoke: Invoke) {
        execute(invoke) {
            val args = invoke.parseArgs(SecretStoreKeyArgs::class.java)
            requireProtocol(args.protocolVersion)
            val value = storage.load(parseKey(args.key))
            try {
                val response = JSObject().put("found", value != null)
                if (value != null) {
                    response.put("valueBase64", Base64.encodeToString(value, Base64.NO_WRAP))
                }
                invoke.resolve(response)
            } finally {
                value?.fill(0)
            }
        }
    }

    @Command
    fun delete(invoke: Invoke) {
        execute(invoke) {
            val args = invoke.parseArgs(SecretStoreKeyArgs::class.java)
            requireProtocol(args.protocolVersion)
            storage.delete(parseKey(args.key))
            invoke.resolve()
        }
    }

    @Command
    fun logout(invoke: Invoke) {
        execute(invoke) {
            requireProtocol(invoke.parseArgs(SecretStoreHandshakeArgs::class.java).protocolVersion)
            storage.logout()
            invoke.resolve()
        }
    }

    private inline fun execute(invoke: Invoke, action: () -> Unit) {
        try {
            action()
        } catch (error: AndroidSecretStoreException) {
            invoke.reject(error.error.code, error.error.code)
        } catch (_: Exception) {
            val error = AndroidSecretStoreError.StorageFailure
            invoke.reject(error.code, error.code)
        }
    }

    private fun requireProtocol(protocolVersion: Int) {
        if (protocolVersion != PROTOCOL_VERSION) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.Unavailable)
        }
    }

    private fun parseKey(key: String): AndroidSecretKey =
        when (key) {
            AndroidSecretKey.AccessToken.storageName -> AndroidSecretKey.AccessToken
            AndroidSecretKey.RefreshToken.storageName -> AndroidSecretKey.RefreshToken
            AndroidSecretKey.SubscriptionCredential.storageName ->
                AndroidSecretKey.SubscriptionCredential
            else -> throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)
        }

    private fun decodeValue(encoded: String): ByteArray {
        if (encoded.isEmpty() || encoded.length > MAX_BASE64_SECRET_CHARS) {
            throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)
        }
        val decoded =
            try {
                Base64.decode(encoded, Base64.NO_WRAP)
            } catch (_: IllegalArgumentException) {
                throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)
            }
        val supplied = encoded.toByteArray(Charsets.US_ASCII)
        val canonical = Base64.encode(decoded, Base64.NO_WRAP)
        try {
            if (!supplied.contentEquals(canonical)) {
                decoded.fill(0)
                throw AndroidSecretStoreException(AndroidSecretStoreError.InvalidValue)
            }
        } finally {
            supplied.fill(0)
            canonical.fill(0)
        }
        return decoded
    }

    private companion object {
        const val PROTOCOL_VERSION = 1
        const val MAX_BASE64_SECRET_CHARS = ((16 * 1024 + 2) / 3) * 4
    }
}
