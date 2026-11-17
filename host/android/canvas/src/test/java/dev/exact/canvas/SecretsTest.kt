package dev.exact.canvas

import java.util.Base64
import javax.crypto.KeyGenerator
import org.junit.Assert.*
import org.junit.Test

class SecretsTest {
    private fun key() = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
    private fun refuses(run: () -> Unit) {
        try { run(); fail("opened an invalid envelope") } catch (_: Exception) { }
    }

    @Test fun valuesRoundTripAcrossInstancesWithFreshCiphertexts() {
        val wrapping = key()
        val first = SecretEnvelope { wrapping }
        val next = SecretEnvelope { wrapping }
        for (value in listOf("", "Authorization: Bearer secret", "query=☃\u0000🔑")) {
            val sealed = first.seal("store/token", value)
            assertEquals(value, next.open("store/token", sealed))
            assertNotEquals(sealed, first.seal("store/token", value))
            if (value.isNotEmpty()) assertFalse(sealed.contains(value))
        }
    }

    @Test fun everyEnvelopePartIsAuthenticated() {
        val wrapping = key()
        val codec = SecretEnvelope { wrapping }
        val bytes = Base64.getDecoder().decode(codec.seal("store/token", "secret"))
        for (index in bytes.indices) {
            val changed = bytes.clone().apply { this[index] = (this[index].toInt() xor 1).toByte() }
            refuses { codec.open("store/token", Base64.getEncoder().encodeToString(changed)) }
        }
        for (size in bytes.indices) refuses {
            codec.open("store/token", Base64.getEncoder().encodeToString(bytes.copyOf(size)))
        }
    }

    @Test fun namesAndStoresCannotExchangeCredentials() {
        val wrapping = key()
        val codec = SecretEnvelope { wrapping }
        val sealed = codec.seal("store/token", "secret")
        refuses { codec.open("store/other", sealed) }
        refuses { codec.open("another/token", sealed) }
        refuses { SecretEnvelope { key() }.open("store/token", sealed) }
    }

    @Test fun aReadNeverCreatesAReplacementWrappingKey() {
        val wrapping = key()
        val sealed = SecretEnvelope { wrapping }.seal("store/token", "secret")
        var asked = false
        refuses {
            SecretEnvelope { create ->
                asked = true
                assertFalse(create)
                throw IllegalStateException("key unavailable")
            }.open("store/token", sealed)
        }
        assertTrue(asked)
        refuses { SecretEnvelope { throw IllegalStateException("locked") }.seal("store/token", "secret") }
    }
}
