// Languages for @exact/translation on Android (LLP 1133.027 D7, §9): the tag table every native
// implementation shares (apple/Languages.swift), ML Kit's languages in its terms, detection's results by
// the specification's algorithm, and the agent's fixed list and detection rule.
package exact.translation

import java.lang.Character.UnicodeScript
import java.util.IllformedLocaleException
import java.util.Locale

internal object Languages {
    /** The agent's languages, every pair of two distinct ones `available` (§9). */
    val agent = listOf("ar", "de", "en", "es", "fr", "hi", "it", "ja", "ko", "pt", "ru", "zh-Hans")

    private val languageId = Regex("(?:[a-z]{2,3}|[a-z]{5,8})(?:-[a-z]{4})?(?:-(?:[a-z]{2}|[0-9]{3}))?(?:-(?:[a-z0-9]{5,8}|[0-9][a-z0-9]{3}))*")

    /** `tag` canonicalised as BCP 47 is (`zh_tw` is `zh-TW`), or null for no tag. */
    fun canonical(tag: String): String? {
        return try {
            val input = tag.replace('_', '-')
            val subtags = input.lowercase(Locale.ROOT).split('-')
            val base = subtags.takeWhile { it.length != 1 }
            if (!languageId.matches(base.joinToString("-"))) return null
            // Locale.Builder silently drops repeated extensions and accepts repeated variants.
            val variants = base.drop(1).filter { it.length >= 5 || (it.length == 4 && it[0].isDigit()) }
            val extensions = subtags.drop(base.size).takeWhile { it != "x" }.filter { it.length == 1 }
            if (variants.distinct() != variants || extensions.distinct() != extensions) return null
            val locale = Locale.Builder().setLanguageTag(input).build()
            locale.toLanguageTag().takeIf { locale.language.isNotEmpty() || input.substringBefore('-').equals("und", true) }
        } catch (_: IllformedLocaleException) { null }
    }

    /** An explicit script takes precedence over the script implied by a Chinese region. */
    fun scripted(tag: String): String {
        val locale = Locale.forLanguageTag(tag)
        val language = locale.toLanguageTag().substringBefore('-')
        val script = locale.script.ifEmpty {
            if (language != "zh") return language
            if (locale.country in setOf("TW", "HK", "MO")) "Hant" else "Hans"
        }
        return "$language-$script"
    }

    /** `tag` truncated as LLP 1060 truncates until `listed` has it, or null. */
    fun listed(tag: String, listed: Collection<String>): String? {
        val explicit = scripted(tag)
        if (explicit in listed) return explicit
        val base = explicit.substringBefore('-')
        // A fallback from zh-Hant to a bare zh model would translate Traditional as Simplified.
        if (base == "zh") return null
        return (spelled[base] ?: base).takeIf { it in listed }
    }

    // ML Kit names a language by its own code (TranslateLanguage), one per language: its Chinese is
    // Simplified Chinese, so it is listed as `zh-Hans` and `zh-Hant` truncates to nothing. A tag BCP 47
    // spells otherwise than ML Kit does is listed under BCP 47's spelling.
    // https://developers.google.com/ml-kit/language/translation/translation-language-support
    private val spelled = mapOf("zh" to "zh-Hans", "no" to "nb", "tl" to "fil")

    /** ML Kit's languages as D7's table lists them. */
    fun mlkitListed(codes: Collection<String>): List<String> = codes.map { spelled[it] ?: it }

    /** The ML Kit code of a listed language. */
    fun mlkitCode(listed: String): String = spelled.entries.firstOrNull { it.value == listed }?.key ?: listed

    // The bundled detector has no API to enumerate its languages. Regional and script variants
    // match their base language. https://developers.google.com/ml-kit/language/identification/langid-support
    private val identifiable = """
        af am ar az be bg bn bs ca ceb co cs cy da de el en eo es et eu fa fi fil fr fy ga gd gl gu ha
        haw he hi hmn hr ht hu hy id ig is it ja jv ka kk km kn ko ku ky la lb lo lt lv mg mi mk ml mn
        mr ms mt my ne nl no ny pa pl ps pt ro ru sd si sk sl sm sn so sq sr st su sv sw ta te tg th tr
        uk ur uz vi xh yi yo zh zu
    """.trimIndent().split(Regex("\\s+")).toSet()

    fun identifies(tag: String): Boolean {
        val base = canonical(tag)?.substringBefore('-') ?: return false
        return when (base) {
            "nb" -> "no"
            "tl" -> "fil"
            else -> base
        } in identifiable
    }

    /**
     * Detection's results by the specification's algorithm (D7): the platform's probabilities sorted by
     * confidence, those below the unknown share dropped, stopping once they sum to 0.99, and `und` last with
     * the unknown share.
     */
    fun results(hypotheses: Map<String, Double>): List<Pair<String, Double>> {
        val known = hypotheses - "und"
        val unknown = hypotheses["und"] ?: (1 - known.values.sum()).coerceAtLeast(0.0)
        val out = ArrayList<Pair<String, Double>>()
        var sum = 0.0
        for ((language, confidence) in known.entries.sortedByDescending { it.value }) {
            if (confidence == 0.0 || confidence < unknown || sum >= 0.99) break
            out += language to confidence
            sum += confidence
        }
        out += "und" to unknown
        return out
    }

    /**
     * The agent's detection (§9): any kana is Japanese, then any Hangul Korean, else the first letter's
     * script; null where there is no letter.
     */
    fun ruled(input: String): String? {
        var first: UnicodeScript? = null
        var hangul = false
        val points = input.codePoints().iterator()
        while (points.hasNext()) {
            val point = points.nextInt()
            if (!Character.isLetter(point)) continue
            val script = UnicodeScript.of(point)
            if (script == UnicodeScript.HIRAGANA || script == UnicodeScript.KATAKANA) return "ja"
            if (script == UnicodeScript.HANGUL) hangul = true
            if (first == null) first = script
        }
        if (hangul) return "ko"
        return when (first) {
            UnicodeScript.HAN -> "zh"
            UnicodeScript.CYRILLIC -> "ru"
            UnicodeScript.ARABIC -> "ar"
            UnicodeScript.DEVANAGARI -> "hi"
            UnicodeScript.LATIN -> "en"
            else -> null
        }
    }
}
