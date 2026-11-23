package app.nocturne.system

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * A secret the app keeps, such as the key to the person's online service. It is sealed by a key
 * Android's keystore holds and never hands out, and the sealed copy sits in the app's folder that
 * no backup or device-to-device copy takes, so it never leaves this phone. Forgetting removes
 * both.
 */
class Safe(private val dir: File) {
    companion object {
        private const val STORE = "AndroidKeyStore"
        private const val CIPHER = "AES/GCM/NoPadding"
        private const val TAG_BITS = 128

        /** A secret's name: a few plain letters, so it is a file name and an alias as it is. */
        fun validName(name: String): Boolean =
            name.length in 1..32 && name.all { it in 'a'..'z' || it == '-' }

        /** The sealed form on disk: the nonce's length, the nonce, then the sealed bytes. */
        fun pack(iv: ByteArray, sealed: ByteArray): ByteArray {
            require(iv.size in 1..255)
            return byteArrayOf(iv.size.toByte()) + iv + sealed
        }

        /** The nonce and the sealed bytes, or null when the file is not one this wrote. */
        fun unpack(packed: ByteArray): Pair<ByteArray, ByteArray>? {
            if (packed.isEmpty()) return null
            val n = packed[0].toInt() and 0xff
            if (n == 0 || packed.size <= 1 + n) return null
            return packed.copyOfRange(1, 1 + n) to packed.copyOfRange(1 + n, packed.size)
        }
    }

    private fun alias(name: String) = "nocturne-$name"

    private fun file(name: String) = File(dir, name)

    private fun keyStore(): KeyStore = KeyStore.getInstance(STORE).apply { load(null) }

    private fun sealingKey(name: String): SecretKey {
        (keyStore().getKey(alias(name), null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, STORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                alias(name),
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return generator.generateKey()
    }

    fun keep(name: String, value: String) {
        require(validName(name))
        // Nothing of an earlier secret stays: a new sealing key for each.
        forget(name)
        val cipher = Cipher.getInstance(CIPHER)
        cipher.init(Cipher.ENCRYPT_MODE, sealingKey(name))
        val sealed = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        dir.mkdirs()
        val staged = File(dir, "$name.new")
        staged.writeBytes(pack(cipher.iv, sealed))
        if (!staged.renameTo(file(name))) {
            staged.delete()
            throw java.io.IOException("not kept")
        }
    }

    /** The secret, or null when none is kept or it can no longer be opened. */
    fun read(name: String): String? {
        require(validName(name))
        val packed = file(name).takeIf { it.isFile }?.readBytes() ?: return null
        val (iv, sealed) = unpack(packed) ?: return null
        val key = keyStore().getKey(alias(name), null) as? SecretKey ?: return null
        return try {
            val cipher = Cipher.getInstance(CIPHER)
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, iv))
            String(cipher.doFinal(sealed), Charsets.UTF_8)
        } catch (e: java.security.GeneralSecurityException) {
            null
        }
    }

    fun forget(name: String) {
        require(validName(name))
        file(name).delete()
        val store = keyStore()
        if (store.containsAlias(alias(name))) store.deleteEntry(alias(name))
    }
}
