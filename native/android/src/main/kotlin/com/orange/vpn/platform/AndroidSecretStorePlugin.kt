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
            AndroidSecretStoreProtocol.requireVersion(
                invoke.parseArgs(SecretStoreHandshakeArgs::class.java).protocolVersion
            )
            val response =
                JSObject().put("protocolVersion", AndroidSecretStoreProtocol.VERSION)
            invoke.resolve(response)
        }
    }

    @Command
    fun store(invoke: Invoke) {
        execute(invoke) {
            val args = invoke.parseArgs(StoreSecretArgs::class.java)
            AndroidSecretStoreProtocol.requireVersion(args.protocolVersion)
            val value = decodeValue(args.valueBase64)
            try {
                storage.store(AndroidSecretStoreProtocol.parseKey(args.key), value)
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
            AndroidSecretStoreProtocol.requireVersion(args.protocolVersion)
            val value = storage.load(AndroidSecretStoreProtocol.parseKey(args.key))
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
            AndroidSecretStoreProtocol.requireVersion(args.protocolVersion)
            storage.delete(AndroidSecretStoreProtocol.parseKey(args.key))
            invoke.resolve()
        }
    }

    @Command
    fun logout(invoke: Invoke) {
        execute(invoke) {
            AndroidSecretStoreProtocol.requireVersion(
                invoke.parseArgs(SecretStoreHandshakeArgs::class.java).protocolVersion
            )
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

    private fun decodeValue(encoded: String): ByteArray = AndroidSecretStoreProtocol.decodeValue(
        encoded,
        { Base64.decode(it, Base64.NO_WRAP) },
        { Base64.encode(it, Base64.NO_WRAP) }
    )
}
