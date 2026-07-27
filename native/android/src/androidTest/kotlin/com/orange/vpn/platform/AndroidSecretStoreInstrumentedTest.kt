package com.orange.vpn.platform

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.security.KeyStore

@RunWith(AndroidJUnit4::class)
class AndroidSecretStoreInstrumentedTest {
    private lateinit var storage: AndroidSecretStore

    @Before
    fun setUp() {
        storage = AndroidSecretStore(InstrumentationRegistry.getInstrumentation().targetContext)
        storage.logout()
    }

    @After
    fun tearDown() {
        storage.logout()
    }

    @Test
    fun keystoreRoundTripOverwriteAndLogout() {
        val oldAccess = byteArrayOf(0x11, 0x12, 0x13)
        storage.store(AndroidSecretKey.AccessToken, oldAccess)
        assertTrue(oldAccess.all { it == 0.toByte() })

        val access = byteArrayOf(0x21, 0x22, 0x23, 0x24)
        val refresh = byteArrayOf(0x31, 0x32, 0x33, 0x34)
        val subscription = byteArrayOf(0x41, 0x42, 0x43, 0x44)
        storage.store(AndroidSecretKey.AccessToken, access)
        storage.store(AndroidSecretKey.RefreshToken, refresh)
        storage.store(AndroidSecretKey.SubscriptionCredential, subscription)
        assertTrue(access.all { it == 0.toByte() })
        assertTrue(refresh.all { it == 0.toByte() })
        assertTrue(subscription.all { it == 0.toByte() })

        val loaded = storage.load(AndroidSecretKey.AccessToken)
        assertArrayEquals(byteArrayOf(0x21, 0x22, 0x23, 0x24), loaded)
        loaded?.fill(0)

        storage.logout()
        assertNull(storage.load(AndroidSecretKey.AccessToken))
        assertNull(storage.load(AndroidSecretKey.RefreshToken))
        assertNull(storage.load(AndroidSecretKey.SubscriptionCredential))
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        assertFalse(keyStore.containsAlias("com.orange.vpn.secret-storage.v1"))
    }

    @Test
    fun invalidValueIsRejectedAndCleared() {
        val oversized = ByteArray(16 * 1024 + 1) { 0x5a }
        val error =
            assertThrows(AndroidSecretStoreException::class.java) {
                storage.store(AndroidSecretKey.AccessToken, oversized)
            }
        assertEquals(AndroidSecretStoreError.InvalidValue, error.error)
        assertEquals("secret-invalid-value", error.message)
        assertTrue(oversized.all { it == 0.toByte() })
    }

    @Test
    fun ciphertextCannotBeMovedBetweenTokenKeys() {
        val access = byteArrayOf(0x41, 0x42, 0x43)
        val refresh = byteArrayOf(0x51, 0x52, 0x53)
        storage.store(AndroidSecretKey.AccessToken, access)
        storage.store(AndroidSecretKey.RefreshToken, refresh)

        val preferences =
            InstrumentationRegistry.getInstrumentation()
                .targetContext
                .getSharedPreferences("orange.secure-secrets.v1", Context.MODE_PRIVATE)
        val refreshCiphertext = preferences.getString("orange.refresh-token", null)
        assertTrue(
            preferences.edit().putString("orange.access-token", refreshCiphertext).commit(),
        )

        val error =
            assertThrows(AndroidSecretStoreException::class.java) {
                storage.load(AndroidSecretKey.AccessToken)
            }
        assertEquals(AndroidSecretStoreError.StorageFailure, error.error)
        assertEquals("secret-store-failure", error.message)
    }

    @Test
    fun rustPluginRoundTripUsesKeystoreBackendAndCleansUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val bridgeTestPreferences =
            context.getSharedPreferences(BRIDGE_TEST_PREFERENCES, Context.MODE_PRIVATE)

        try {
            assertTrue(bridgeTestPreferences.getBoolean(BRIDGE_TEST_COMPLETED, false))
            assertNull(storage.load(AndroidSecretKey.AccessToken))
            assertNull(storage.load(AndroidSecretKey.RefreshToken))
            assertNull(storage.load(AndroidSecretKey.SubscriptionCredential))
            val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
            assertFalse(keyStore.containsAlias("com.orange.vpn.secret-storage.v1"))
        } finally {
            assertTrue(bridgeTestPreferences.edit().clear().commit())
        }
    }
}
