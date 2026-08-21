package com.orange.vpn.platform

import android.app.Activity
import android.util.Base64
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.orange.vpn.controlplane.mobile.Client
import com.orange.vpn.controlplane.mobile.Mobile
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class ControlPlanePayloadArgs {
    var protocolVersion: Int = 0
    lateinit var payloadBase64: String
}

@InvokeArg
class ControlPlaneVersionArgs {
    var protocolVersion: Int = 0
}

@TauriPlugin
class AndroidControlPlanePlugin(private val activity: Activity) : Plugin(activity) {
    private var client: Client? = null
    private val cacheFile = File(activity.filesDir, "bootstrap.cache")
    private val previousCacheFile = File(activity.filesDir, "bootstrap.cache.previous")

    @Synchronized
    @Command
    fun configure(invoke: Invoke) {
        execute(invoke) {
            val args = invoke.parseArgs(ControlPlanePayloadArgs::class.java)
            requireVersion(args.protocolVersion)
            val payload = decode(args.payloadBase64)
            try {
                client?.close()
                client = Mobile.newClient(payload)
            } finally {
                payload.fill(0)
            }
            invoke.resolve()
        }
    }

    @Synchronized
    @Command
    fun executeRequest(invoke: Invoke) {
        execute(invoke) {
            val args = invoke.parseArgs(ControlPlanePayloadArgs::class.java)
            requireVersion(args.protocolVersion)
            val payload = decode(args.payloadBase64)
            val response = try {
                client?.execute(payload) ?: throw IllegalStateException("bootstrap-unavailable")
            } finally {
                payload.fill(0)
            }
            try {
                invoke.resolve(
                    JSObject().put(
                        "payloadBase64",
                        Base64.encodeToString(response, Base64.NO_WRAP)
                    )
                )
            } finally {
                response.fill(0)
            }
        }
    }

    @Synchronized
    @Command
    fun close(invoke: Invoke) {
        execute(invoke) {
            client?.close()
            client = null
            invoke.resolve()
        }
    }

    @Synchronized
    @Command
    fun loadCache(invoke: Invoke) {
        execute(invoke) {
            requireVersion(
                invoke.parseArgs(ControlPlaneVersionArgs::class.java).protocolVersion
            )
            for (file in listOf(cacheFile, previousCacheFile)) {
                if (!file.isFile) continue
                try {
                    val plaintext = decryptCache(file.readBytes())
                    try {
                        invoke.resolve(
                            JSObject()
                                .put("found", true)
                                .put("payloadBase64", Base64.encodeToString(plaintext, Base64.NO_WRAP))
                        )
                        return@execute
                    } finally {
                        plaintext.fill(0)
                    }
                } catch (_: Exception) {
                    // Try the rollback slot.
                }
            }
            invoke.resolve(JSObject().put("found", false))
        }
    }

    @Synchronized
    @Command
    fun storeCache(invoke: Invoke) {
        execute(invoke) {
            val args = invoke.parseArgs(ControlPlanePayloadArgs::class.java)
            requireVersion(args.protocolVersion)
            val plaintext = decode(args.payloadBase64)
            require(plaintext.size <= 256 * 1024)
            val sealed = try {
                encryptCache(plaintext)
            } finally {
                plaintext.fill(0)
            }
            val candidate = File(activity.filesDir, "bootstrap.cache.candidate")
            try {
                candidate.outputStream().use { stream ->
                    stream.write(sealed)
                    stream.fd.sync()
                }
                previousCacheFile.delete()
                if (cacheFile.exists() && !cacheFile.renameTo(previousCacheFile)) {
                    throw IllegalStateException("cache rotation failed")
                }
                if (!candidate.renameTo(cacheFile)) {
                    throw IllegalStateException("cache promotion failed")
                }
            } finally {
                sealed.fill(0)
                candidate.delete()
            }
            invoke.resolve()
        }
    }

    private fun requireVersion(version: Int) {
        if (version != 2) throw IllegalArgumentException("invalid-request")
    }

    private fun decode(value: String): ByteArray {
        if (value.isEmpty() || value.length > 3 * 1024 * 1024) {
            throw IllegalArgumentException("invalid-request")
        }
        return Base64.decode(value, Base64.NO_WRAP)
    }

    private fun encryptCache(plaintext: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, cacheKey())
        val ciphertext = cipher.doFinal(plaintext)
        return byteArrayOf(1, cipher.iv.size.toByte()) + cipher.iv + ciphertext
    }

    private fun decryptCache(sealed: ByteArray): ByteArray {
        require(sealed.size in 32..(300 * 1024))
        require(sealed[0].toInt() == 1)
        val ivLength = sealed[1].toInt() and 0xff
        require(ivLength in 12..16 && sealed.size > 2 + ivLength + 16)
        val iv = sealed.copyOfRange(2, 2 + ivLength)
        val ciphertext = sealed.copyOfRange(2 + ivLength, sealed.size)
        try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, cacheKey(), GCMParameterSpec(128, iv))
            return cipher.doFinal(ciphertext)
        } finally {
            sealed.fill(0)
            iv.fill(0)
            ciphertext.fill(0)
        }
    }

    private fun cacheKey(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val alias = "orange.bootstrap.cache.v1"
        (keyStore.getKey(alias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(
                alias,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return generator.generateKey()
    }

    private inline fun execute(invoke: Invoke, action: () -> Unit) {
        try {
            action()
        } catch (error: Exception) {
            val code = error.message?.takeIf {
                it in setOf(
                    "invalid-request",
                    "bootstrap-unavailable",
                    "timeout",
                    "canceled",
                    "dns-failure",
                    "tls-failure",
                    "response-too-large"
                )
            } ?: "bootstrap-unavailable"
            invoke.reject(code, code)
        }
    }
}
