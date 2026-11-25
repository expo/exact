package exact.translation

import com.google.mlkit.nl.translate.TranslateLanguage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** D7's table on Android, against ML Kit's own language list (no model is loaded). */
class TranslationTest {
    private val listed = Languages.mlkitListed(TranslateLanguage.getAllLanguages())
    private fun code(tag: String) = Languages.listed(Languages.canonical(tag)!!, listed)?.let(Languages::mlkitCode)

    @Test fun canonicalisesAsBcp47() {
        assertEquals("zh-TW", Languages.canonical("zh_tw"))
        assertEquals("sr-Latn-RS", Languages.canonical("SR-latn-rs"))
        assertNull(Languages.canonical(""))
        assertNull(Languages.canonical("e"))
        assertNull(Languages.canonical("en--US"))
        assertNull(Languages.canonical("en-a"))
        assertNull(Languages.canonical("en-a-foo-A-bar"))
        assertNull(Languages.canonical("en-1901-1901"))
        assertNull(Languages.canonical("en-abc"))
        assertNull(Languages.canonical("i-klingon"))
        assertEquals("en-x-a-a", Languages.canonical("en-x-a-a"))
        assertEquals("und", Languages.canonical("und"))
    }

    @Test fun scriptsChineseByRegion() {
        assertEquals("zh-Hans", Languages.scripted("zh"))
        assertEquals("zh-Hans", Languages.scripted("zh-CN"))
        assertEquals("zh-Hant", Languages.scripted("zh-TW"))
        assertEquals("zh-Hant", Languages.scripted("zh-HK"))
        assertEquals("en", Languages.scripted("en-GB"))
    }

    @Test fun mapsTagsToMlKitCodes() {
        assertEquals(TranslateLanguage.JAPANESE, code("ja"))
        assertEquals(TranslateLanguage.ENGLISH, code("en-US"))
        assertEquals(TranslateLanguage.SPANISH, code("es-AR"))
        // ML Kit's Chinese is Simplified: Traditional truncates to nothing rather than to it.
        assertEquals(TranslateLanguage.CHINESE, code("zh"))
        assertEquals(TranslateLanguage.CHINESE, code("zh-CN"))
        assertNull(code("zh-TW"))
        assertNull(code("zh-Hant"))
        assertNull(code("zh-TW-u-ca-gregory"))
        assertNull(Languages.listed("zh-Hant", Languages.agent))
        assertEquals("zh-Hans", Languages.listed("zh-CN", Languages.agent))
        // BCP 47's spelling where ML Kit's differs.
        assertEquals(TranslateLanguage.NORWEGIAN, code("nb"))
        assertEquals(TranslateLanguage.TAGALOG, code("fil"))
        assertEquals(TranslateLanguage.NORWEGIAN, code("no-Latn"))
        assertEquals(TranslateLanguage.NORWEGIAN, code("no"))
        assertEquals(TranslateLanguage.TAGALOG, code("tl"))
        assertNull(code("xx"))
    }

    @Test fun resultsFollowTheSpecification() {
        val r = Languages.results(mapOf("ja" to 0.9, "zh" to 0.05, "ko" to 0.01))
        assertEquals(listOf("ja", "zh", "und"), r.map { it.first })
        assertEquals(0.04, r.last().second, 1e-9)
        // Stops once the listed ones sum to 0.99.
        assertEquals(listOf("ru", "und"), Languages.results(mapOf("ru" to 0.995, "uk" to 0.004)).map { it.first })
        assertEquals(listOf("und" to 1.0), Languages.results(emptyMap()))
        assertEquals(listOf("ja" to 0.70, "en" to 0.12, "und" to 0.10),
            Languages.results(mapOf("ja" to 0.70, "en" to 0.12, "und" to 0.10)))
        assertEquals(listOf("ja" to 0.4, "und" to 0.0),
            Languages.results(mapOf("ja" to 0.4, "en" to 0.0, "und" to 0.0)))
        val explicit = Languages.results(mapOf("ja" to 0.75, "ru" to 0.05, "und" to 0.2))
        assertEquals(listOf("ja" to 0.75, "und" to 0.2), explicit)
    }

    @Test fun agentRuleReadsScripts() {
        assertEquals("ja", Languages.ruled("明日の会議は十時からです。"))
        assertEquals("zh", Languages.ruled("明日会议"))
        assertEquals("ko", Languages.ruled("안녕하세요"))
        assertEquals("ru", Languages.ruled("Привет"))
        assertEquals("en", Languages.ruled("¿Qué tal?"))
        assertNull(Languages.ruled("123 !"))
        assertNull(Languages.ruled("・"))
        assertEquals("ru", Languages.ruled("Ꙁ"))
    }
}
