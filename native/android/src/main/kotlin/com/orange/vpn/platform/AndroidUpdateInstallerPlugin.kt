package com.orange.vpn.platform

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.activity.result.ActivityResult
import androidx.core.content.FileProvider
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.security.MessageDigest

@InvokeArg
class InstallUpdateArgs {
    var protocolVersion: Int = 0
    lateinit var apkPath: String
    lateinit var sha256: String
    var expectedBytes: Long = 0
    lateinit var packageName: String
    var versionCode: Long = 0
    lateinit var certificateSha256: String
}

@InvokeArg
class UpdateProtocolArgs {
    var protocolVersion: Int = 0
}

@TauriPlugin
class AndroidUpdateInstallerPlugin(private val activity: Activity) : Plugin(activity) {
    private val updateDirectory = File(activity.cacheDir, "updates")

    init {
        updateDirectory.deleteRecursively()
    }

    @Command
    fun prepare(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(UpdateProtocolArgs::class.java)
            require(args.protocolVersion == 1)
            if (
                Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
                !activity.packageManager.canRequestPackageInstalls()
            ) {
                activity.startActivity(
                    Intent(
                        Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                        Uri.parse("package:${activity.packageName}")
                    )
                )
                invoke.resolve(JSObject().put("permissionRequired", true).put("apkPath", ""))
                return
            }
            updateDirectory.deleteRecursively()
            require(updateDirectory.mkdirs())
            val apk = File(updateDirectory, "orange-update.apk")
            invoke.resolve(
                JSObject().put("permissionRequired", false).put("apkPath", apk.absolutePath)
            )
        } catch (_: Exception) {
            updateDirectory.deleteRecursively()
            invoke.reject("android-update-failed", "android-update-failed")
        }
    }

    @Command
    fun cleanup(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(UpdateProtocolArgs::class.java)
            require(args.protocolVersion == 1)
            updateDirectory.deleteRecursively()
            invoke.resolve(JSObject())
        } catch (_: Exception) {
            invoke.reject("android-update-failed", "android-update-failed")
        }
    }

    @Command
    fun install(invoke: Invoke) {
        Thread {
            try {
                val args = invoke.parseArgs(InstallUpdateArgs::class.java)
                validateArgs(args)
                val apk = File(args.apkPath).canonicalFile
                require(apk == File(updateDirectory, "orange-update.apk").canonicalFile)
                require(apk.isFile && apk.length() == args.expectedBytes)
                verifyHash(apk, args.sha256)
                verifyArchive(apk, args)
                val uri = FileProvider.getUriForFile(
                    activity,
                    "${activity.packageName}.updates",
                    apk
                )
                @Suppress("DEPRECATION")
                val intent = Intent(Intent.ACTION_INSTALL_PACKAGE).apply {
                    setDataAndType(uri, "application/vnd.android.package-archive")
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    putExtra(Intent.EXTRA_RETURN_RESULT, true)
                }
                Handler(Looper.getMainLooper()).post {
                    try {
                        startActivityForResult(invoke, intent, "onInstallResult")
                        Handler(Looper.getMainLooper()).postDelayed(
                            { updateDirectory.deleteRecursively() },
                            30 * 60 * 1000L
                        )
                        invoke.resolve(
                            JSObject().put("permissionRequired", false).put("started", true)
                        )
                    } catch (_: Exception) {
                        updateDirectory.deleteRecursively()
                        invoke.reject("android-update-failed", "android-update-failed")
                    }
                }
            } catch (_: Exception) {
                updateDirectory.deleteRecursively()
                invoke.reject("android-update-failed", "android-update-failed")
            }
        }.start()
    }

    @ActivityCallback
    @Suppress("UNUSED_PARAMETER")
    fun onInstallResult(invoke: Invoke, result: ActivityResult) {
        updateDirectory.deleteRecursively()
    }

    private fun validateArgs(args: InstallUpdateArgs) {
        require(args.protocolVersion == 1)
        require(args.sha256.matches(Regex("^[a-f0-9]{64}$", RegexOption.IGNORE_CASE)))
        require(
            args.certificateSha256.matches(
                Regex("^[a-f0-9]{64}$", RegexOption.IGNORE_CASE)
            )
        )
        require(args.expectedBytes in 1..(512L * 1024 * 1024))
        require(args.packageName == activity.packageName)
        require(args.versionCode > currentVersionCode())
    }

    private fun verifyHash(apk: File, expectedSha256: String) {
        val digest = MessageDigest.getInstance("SHA-256")
        apk.inputStream().use { input ->
            val buffer = ByteArray(64 * 1024)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                digest.update(buffer, 0, count)
            }
            buffer.fill(0)
        }
        require(digest.digest().toHex().equals(expectedSha256, ignoreCase = true))
    }

    @Suppress("DEPRECATION")
    private fun verifyArchive(apk: File, args: InstallUpdateArgs) {
        val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            android.content.pm.PackageManager.GET_SIGNING_CERTIFICATES
        } else {
            android.content.pm.PackageManager.GET_SIGNATURES
        }
        val info = activity.packageManager.getPackageArchiveInfo(apk.absolutePath, flags)
            ?: throw IllegalArgumentException("invalid APK")
        require(info.packageName == args.packageName)
        require(info.longVersionCode == args.versionCode)
        val signatures = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            info.signingInfo?.apkContentsSigners
        } else {
            info.signatures
        } ?: throw IllegalArgumentException("missing APK signature")
        require(signatures.size == 1)
        val digest = MessageDigest.getInstance("SHA-256").digest(signatures[0].toByteArray())
        require(digest.toHex().equals(args.certificateSha256, ignoreCase = true))
    }

    @Suppress("DEPRECATION")
    private fun currentVersionCode(): Long {
        val info = activity.packageManager.getPackageInfo(activity.packageName, 0)
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            info.longVersionCode
        } else {
            info.versionCode.toLong()
        }
    }

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }
}
