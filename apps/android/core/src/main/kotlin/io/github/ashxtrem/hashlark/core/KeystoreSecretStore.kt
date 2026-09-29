// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import io.github.ashxtrem.hashlark.ffi.HashlarkException
import io.github.ashxtrem.hashlark.ffi.SecretStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import java.io.File
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Encrypts short strings with AES-256-GCM. Each value gets its own random
 * IV; the output is `base64(iv ‖ ciphertext ‖ tag)`.
 */
class AesGcmBox(private val key: SecretKey) {
    fun seal(plain: String): String {
        // The provider picks the IV: the Android Keystore refuses a caller-supplied one for
        // its keys ("randomized encryption"), and other providers generate a fresh one too.
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key)
        val sealed = cipher.doFinal(plain.toByteArray(Charsets.UTF_8))
        return Base64.getEncoder().encodeToString(cipher.iv + sealed)
    }

    /** The original text, or `null` when the value was not sealed with this key. */
    fun open(sealed: String): String? = try {
        val bytes = Base64.getDecoder().decode(sealed)
        if (bytes.size <= IV_BYTES) {
            null
        } else {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, bytes.copyOfRange(0, IV_BYTES)))
            String(cipher.doFinal(bytes, IV_BYTES, bytes.size - IV_BYTES), Charsets.UTF_8)
        }
    } catch (_: GeneralSecurityException) {
        null
    } catch (_: IllegalArgumentException) {
        null
    }

    private companion object {
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
        const val TAG_BITS = 128
    }
}

/**
 * The core's [SecretStore] on Android: values are sealed with an AES key
 * that lives in the Android Keystore (never in app storage) and kept in a
 * DataStore file. The file is excluded from backups, because a restored copy
 * could not be decrypted without the key.
 */
class KeystoreSecretStore(context: Context) : SecretStore {
    private val box: AesGcmBox by lazy { AesGcmBox(loadOrCreateKey()) }

    // DataStore allows one instance per file for the whole process, so instances of this
    // class (the engine may be opened again after a failed start) share it.
    private val store = stores.computeIfAbsent(File(context.noBackupFilesDir, "secrets.preferences_pb").absolutePath) { path ->
        PreferenceDataStoreFactory.create(produceFile = { File(path) })
    }

    override fun get(key: String): String? = blocking {
        store.data.first()[stringPreferencesKey(key)]?.let(box::open)
    }

    override fun set(key: String, value: String) {
        val sealed = box.seal(value)
        blocking { store.edit { it[stringPreferencesKey(key)] = sealed } }
    }

    override fun delete(key: String) {
        blocking { store.edit { it.remove(stringPreferencesKey(key)) } }
    }

    private fun <T> blocking(block: suspend () -> T): T = try {
        // Called from a native thread, never the main thread.
        runBlocking(Dispatchers.IO) { block() }
    } catch (e: GeneralSecurityException) {
        throw HashlarkException.Failed("internal", "secret storage failed: ${e.message}", emptyList(), null)
    }

    private fun loadOrCreateKey(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEY_STORE).apply { load(null) }
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEY_STORE)
        generator.init(
            KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        val stores = ConcurrentHashMap<String, DataStore<Preferences>>()
        const val ANDROID_KEY_STORE = "AndroidKeyStore"
        const val KEY_ALIAS = "hashlark.secrets.v1"
    }
}
