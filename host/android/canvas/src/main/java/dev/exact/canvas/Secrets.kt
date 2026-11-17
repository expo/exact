package dev.exact.canvas

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.nio.ByteBuffer
import java.security.KeyStore
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/** The shared secret store's encryption, including transfer credentials (LLP 1133.017 D3).
 * Files and migration belong to Rust. Only the wrapping key lives in Android Keystore; it needs no prompt,
 * so a worker can open a credential after the first unlock while the screen is locked. */
object Secrets {
    private const val ALIAS = "exact.secrets.v1"

    @Synchronized private fun key(scope: String, create: Boolean): SecretKey {
        val digest = MessageDigest.getInstance("SHA-256").digest(scope.toByteArray(Charsets.UTF_8))
        val alias = ALIAS + "." + digest.joinToString("") { "%02x".format(it) }
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        check(create) { "the secret wrapping key is unavailable" }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
            generateKey()
        }
    }

    /** JNI deliberately returns no provider message: a failing provider may include its input. */
    @JvmStatic fun crypt(op: Int, context: String, value: String): String = try {
        val codec = SecretEnvelope { create -> key(context.substringBeforeLast('/'), create) }
        when (op) {
            1 -> "ok:" + codec.seal(context, value)
            2 -> "ok:" + codec.open(context, value)
            else -> "err:unknown secret operation"
        }
    } catch (_: Exception) { "err:Android secret encryption is unavailable or the entry is invalid" }
}

/** Version, wrapped data key (12-byte IV + 32-byte key + 16-byte tag), then sealed UTF-8 value.
 * Both seals authenticate the store and name: moving a file cannot rename or transfer a credential. */
internal class SecretEnvelope(private val wrappingKey: (Boolean) -> SecretKey) {
    private val aadPrefix = "exact.secrets.v1\u0000"

    private fun encrypt(key: SecretKey, aad: ByteArray, bytes: ByteArray): ByteArray =
        Cipher.getInstance("AES/GCM/NoPadding").run {
            init(Cipher.ENCRYPT_MODE, key)
            updateAAD(aad)
            iv + doFinal(bytes)
        }

    private fun decrypt(key: SecretKey, aad: ByteArray, bytes: ByteArray): ByteArray {
        require(bytes.size >= 28)
        return Cipher.getInstance("AES/GCM/NoPadding").run {
            init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, bytes, 0, 12))
            updateAAD(aad)
            doFinal(bytes, 12, bytes.size - 12)
        }
    }

    fun seal(context: String, value: String): String {
        val data = ByteArray(32).also { SecureRandom().nextBytes(it) }
        val plain = value.toByteArray(Charsets.UTF_8)
        try {
            val aad = (aadPrefix + context).toByteArray(Charsets.UTF_8)
            val wrapped = encrypt(wrappingKey(true), aad, data)
            val sealed = encrypt(SecretKeySpec(data, "AES"), aad, plain)
            return Base64.getEncoder().encodeToString(byteArrayOf(1) + wrapped + sealed)
        } finally { data.fill(0); plain.fill(0) }
    }

    fun open(context: String, value: String): String {
        val bytes = Base64.getDecoder().decode(value)
        require(bytes.size >= 89 && bytes[0] == 1.toByte())
        val aad = (aadPrefix + context).toByteArray(Charsets.UTF_8)
        val data = decrypt(wrappingKey(false), aad, bytes.copyOfRange(1, 61))
        try {
            require(data.size == 32)
            val plain = decrypt(SecretKeySpec(data, "AES"), aad, bytes.copyOfRange(61, bytes.size))
            try { return Charsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(plain)).toString() }
            finally { plain.fill(0) }
        } finally { data.fill(0) }
    }
}
