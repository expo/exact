package dev.exact.heavybench

import android.content.Context
import android.util.JsonReader

// The heavy list's data (bench/heavy-list/SPEC.md, "Data"), its colours and live mode, shared by the Views and Compose apps.

class Run(val t: String, val s: String?)
class Photo(val src: String, val w: Int, val h: Int)
class LinkCard(val thumb: String, val title: String, val description: String, val site: String)
class Quote(val id: String, val author: String, val excerpt: String)
class Reaction(val emoji: String, val count: Int)

data class Message(
    val id: String,
    val index: Int,
    val author: String,
    val avatar: String,
    val minutesAgo: Int,
    val paragraphs: List<List<Run>>,
    val photos: List<Photo>?,
    val link: LinkCard?,
    val quote: Quote?,
    val reactions: List<Reaction>?,
    /** Seconds after launch at which `minutesAgo` was true (live inserts). */
    val bornAt: Int = 0,
) {
    fun ago(seconds: Int): String {
        val minutes = minutesAgo + (seconds - bornAt) / 60
        return when {
            minutes < 60 -> "${minutes}m ago"
            minutes < 60 * 24 -> "${minutes / 60}h ago"
            else -> "${minutes / (60 * 24)}d ago"
        }
    }

    /** The same message with reaction [i] one higher (a tap, or live mode's bump). */
    fun bumped(i: Int) = copy(reactions = reactions!!.mapIndexed { j, r -> if (j == i) Reaction(r.emoji, r.count + 1) else r })
}

object Colors {
    const val HAIRLINE = 0xFFE5E5EA.toInt()
    const val SECONDARY = 0xFF8E8E93.toInt()
    const val QUOTE_BAR = 0xFFC7C7CC.toInt()
    const val FILL = 0xFFF2F2F7.toInt()
    const val LABEL2 = 0xFF3C3C43.toInt()
    const val CARD_BORDER = 0xFFD1D1D6.toInt()
    const val LINK = 0xFF007AFF.toInt()
    const val TAG = 0xFF5856D6.toInt()
}

/** `assets/messages.json`, read with the platform's streaming reader. */
fun loadMessages(context: Context): ArrayList<Message> {
    val out = ArrayList<Message>(10_000)
    JsonReader(context.assets.open("messages.json").bufferedReader()).use { r ->
        r.beginObject()
        while (r.hasNext()) {
            if (r.nextName() != "messages") { r.skipValue(); continue }
            r.beginArray()
            while (r.hasNext()) out.add(message(r))
            r.endArray()
        }
        r.endObject()
    }
    return out
}

private inline fun <T> JsonReader.array(item: (JsonReader) -> T): List<T> {
    val out = ArrayList<T>()
    beginArray(); while (hasNext()) out.add(item(this)); endArray()
    return out
}

private inline fun JsonReader.fields(field: (String) -> Unit) {
    beginObject(); while (hasNext()) field(nextName()); endObject()
}

private fun message(r: JsonReader): Message {
    var id = ""; var index = 0; var author = ""; var avatar = ""; var minutes = 0
    var paragraphs: List<List<Run>> = emptyList(); var photos: List<Photo>? = null
    var link: LinkCard? = null; var quote: Quote? = null; var reactions: List<Reaction>? = null
    r.fields { name ->
        when (name) {
            "id" -> id = r.nextString()
            "index" -> index = r.nextInt()
            "author" -> author = r.nextString()
            "avatar" -> avatar = r.nextString()
            "minutesAgo" -> minutes = r.nextInt()
            "paragraphs" -> paragraphs = r.array { p -> p.array { run(it) } }
            "photos" -> photos = r.array { p ->
                var src = ""; var w = 0; var h = 0
                p.fields { when (it) { "src" -> src = p.nextString(); "w" -> w = p.nextInt(); "h" -> h = p.nextInt(); else -> p.skipValue() } }
                Photo(src, w, h)
            }
            "link" -> {
                var thumb = ""; var title = ""; var description = ""; var site = ""
                r.fields { when (it) { "thumb" -> thumb = r.nextString(); "title" -> title = r.nextString(); "description" -> description = r.nextString(); "site" -> site = r.nextString(); else -> r.skipValue() } }
                link = LinkCard(thumb, title, description, site)
            }
            "quote" -> {
                var qid = ""; var qa = ""; var excerpt = ""
                r.fields { when (it) { "id" -> qid = r.nextString(); "author" -> qa = r.nextString(); "excerpt" -> excerpt = r.nextString(); else -> r.skipValue() } }
                quote = Quote(qid, qa, excerpt)
            }
            "reactions" -> reactions = r.array { p ->
                var emoji = ""; var count = 0
                p.fields { when (it) { "emoji" -> emoji = p.nextString(); "count" -> count = p.nextInt(); else -> p.skipValue() } }
                Reaction(emoji, count)
            }
            else -> r.skipValue()
        }
    }
    return Message(id, index, author, avatar, minutes, paragraphs, photos, link, quote, reactions)
}

private fun run(r: JsonReader): Run {
    var t = ""; var s: String? = null
    r.fields { when (it) { "t" -> t = r.nextString(); "s" -> s = r.nextString(); else -> r.skipValue() } }
    return Run(t, s)
}

/**
 * Live mode's 250 ms step k (SPEC, "Live mode"), as the iOS apps take it: insert a copy of original message
 * (k*37) % 10000 at the top, then bump reaction k % n of original message (k*101) % 10000, or of the next one that
 * has reactions. Returns the bumped message's position, or -1.
 */
fun liveStep(messages: MutableList<Message>, k: Int, seconds: Int): Int {
    // After k-1 inserts, original message j is at k-1+j.
    val base = messages[k - 1 + (k * 37) % 10_000]
    messages.add(0, base.copy(id = "live-$k", index = -k, minutesAgo = 0, bornAt = seconds))
    var i = k + (k * 101) % 10_000
    while (i < messages.size && messages[i].reactions.isNullOrEmpty()) i++
    if (i >= messages.size) return -1
    messages[i] = messages[i].bumped(k % messages[i].reactions!!.size)
    return i
}
