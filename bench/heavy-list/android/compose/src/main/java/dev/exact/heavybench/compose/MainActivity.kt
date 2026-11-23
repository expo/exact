package dev.exact.heavybench.compose

import android.graphics.Bitmap
import android.os.Bundle
import android.view.ViewConfiguration
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.toMutableStateList
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.exact.heavybench.Colors
import dev.exact.heavybench.Images
import dev.exact.heavybench.LinkCard
import dev.exact.heavybench.Message
import dev.exact.heavybench.Photo
import dev.exact.heavybench.Quote
import dev.exact.heavybench.Run
import dev.exact.heavybench.liveStep
import dev.exact.heavybench.loadMessages
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.math.min

private val Hairline = Color(Colors.HAIRLINE)
private val Secondary = Color(Colors.SECONDARY)
private val QuoteBar = Color(Colors.QUOTE_BAR)
private val Fill = Color(Colors.FILL)
private val Label2 = Color(Colors.LABEL2)
private val CardBorder = Color(Colors.CARD_BORDER)
private val Link = Color(Colors.LINK)
private val Tag = Color(Colors.TAG)

/**
 * The heavy list (bench/heavy-list/SPEC.md) in Jetpack Compose, the ordinary way: one LazyColumn keyed by message
 * id over a snapshot state list, one BasicText(AnnotatedString) per paragraph, FlowRow for the chips. Launch extra:
 * `live=1` (live mode).
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        val messages = loadMessages(this).toMutableStateList()
        val live = intent.getStringExtra("live") == "1"
        setContent {
            var seconds by remember { mutableIntStateOf(0) }
            if (live) LaunchedEffect(Unit) {
                var k = 0
                while (true) {
                    delay(250)
                    k++
                    liveStep(messages, k, seconds)
                    if (k % 4 == 0) seconds++
                }
            }
            Column(Modifier.fillMaxSize().background(Color.White).systemBarsPadding()) {
                Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp)) {
                    BasicText("Heavy list", Modifier.weight(1f), TextStyle(fontSize = 17.sp, fontWeight = FontWeight.SemiBold))
                    BasicText(if (live) "Live: on" else "Live: off", style = TextStyle(fontSize = 13.sp, color = Secondary))
                }
                Box(Modifier.fillMaxWidth().height(1.dp / LocalDensity.current.density).background(Hairline))
                val contentWidth = LocalConfiguration.current.screenWidthDp.dp - 84.dp
                val list = rememberLazyListState()
                LazyColumn(Modifier.fillMaxSize().scrollbar(list), list) {
                    items(messages, key = { it.id }) { m ->
                        MessageRow(m, seconds, contentWidth) { r ->
                            val i = messages.indexOfFirst { it.id == m.id }
                            if (i >= 0) messages[i] = messages[i].bumped(r)
                        }
                    }
                }
            }
        }
    }
}

private val Thumb = Color(0x4F000000)

/**
 * The scrollbar a View draws for a list, which LazyColumn has none of (UIKit and SwiftUI lists show theirs too):
 * Theme.Material.Light's thumb, the scrollbar size wide and at least twice that long, shown while the list scrolls
 * and faded after it stops by ViewConfiguration's delay and duration.
 */
@Composable
private fun Modifier.scrollbar(state: LazyListState): Modifier {
    val alpha = remember { Animatable(0f) }
    val width = ViewConfiguration.get(LocalContext.current).scaledScrollBarSize.toFloat()
    LaunchedEffect(state) {
        snapshotFlow { state.isScrollInProgress }.drop(1).collectLatest { scrolling ->
            if (scrolling) alpha.snapTo(1f)
            else {
                delay(ViewConfiguration.getScrollDefaultDelay().toLong())
                alpha.animateTo(0f, tween(ViewConfiguration.getScrollBarFadeDuration(), easing = LinearEasing))
            }
        }
    }
    return drawWithContent {
        drawContent()
        val s = state.scrollIndicatorState ?: return@drawWithContent
        if (alpha.value == 0f || s.contentSize <= s.viewportSize) return@drawWithContent
        val h = maxOf(size.height * s.viewportSize / s.contentSize, 2 * width)
        val top = (size.height - h) * s.scrollOffset / (s.contentSize - s.viewportSize)
        drawRect(Thumb, Offset(size.width - width, top), Size(width, h), alpha.value)
    }
}

@Composable
private fun MessageRow(m: Message, seconds: Int, contentWidth: Dp, react: (Int) -> Unit) {
    val ago = m.ago(seconds)
    Row(
        Modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = false) { contentDescription = "${m.author}, $ago" }
            .drawBehind {
                val h = maxOf(1f, 0.5.dp.toPx())
                drawRect(Hairline, Offset(68.dp.toPx(), size.height - h), Size(size.width - 84.dp.toPx(), h))
            }
            .padding(start = 16.dp, top = 12.dp, end = 16.dp, bottom = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        BundleImage(m.avatar, 40.dp, 40.dp, Modifier.clip(CircleShape))
        Column(Modifier.weight(1f)) {
            Row {
                BasicText(m.author, Modifier.alignByBaseline(), TextStyle(fontSize = 15.sp, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Spacer(Modifier.width(6.dp))
                BasicText(ago, Modifier.alignByBaseline(), TextStyle(fontSize = 13.sp, color = Secondary), maxLines = 1)
            }
            m.quote?.let { QuoteBox(it) }
            Column(Modifier.padding(top = 6.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for (p in m.paragraphs) {
                    val text = remember(p) { paragraph(p) }
                    BasicText(text, style = TextStyle(fontSize = 16.sp))
                }
            }
            m.photos?.takeIf { it.isNotEmpty() }?.let { PhotoGrid(it, contentWidth) }
            m.link?.let { LinkCardView(it, contentWidth) }
            m.reactions?.takeIf { it.isNotEmpty() }?.let { reactions ->
                @OptIn(ExperimentalLayoutApi::class)
                FlowRow(Modifier.padding(top = 8.dp), Arrangement.spacedBy(6.dp), Arrangement.spacedBy(6.dp)) {
                    reactions.forEachIndexed { i, r ->
                        Row(
                            Modifier.height(28.dp).clip(RoundedCornerShape(14.dp)).background(Fill).clickable { react(i) }.padding(horizontal = 10.dp),
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                            verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
                        ) {
                            BasicText(r.emoji, style = TextStyle(fontSize = 14.sp))
                            BasicText(r.count.toString(), style = TextStyle(fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Label2))
                        }
                    }
                }
            }
        }
    }
}

private fun paragraph(runs: List<Run>): AnnotatedString = buildAnnotatedString {
    for (run in runs) {
        val style = when (run.s) {
            "bold" -> SpanStyle(fontWeight = FontWeight.SemiBold)
            "italic" -> SpanStyle(fontStyle = FontStyle.Italic)
            "code" -> SpanStyle(fontFamily = FontFamily.Monospace, fontSize = 15.sp, background = Fill)
            "link" -> SpanStyle(color = Link, textDecoration = TextDecoration.Underline)
            "mention" -> SpanStyle(fontWeight = FontWeight.SemiBold, color = Link)
            "tag" -> SpanStyle(color = Tag)
            else -> null
        }
        if (style == null) append(run.t) else { pushStyle(style); append(run.t); pop() }
    }
}

@Composable
private fun QuoteBox(q: Quote) {
    Column(
        Modifier
            .padding(top = 6.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(Fill)
            .drawBehind { drawRect(QuoteBar, size = Size(3.dp.toPx(), size.height)) }
            .padding(start = 11.dp, top = 8.dp, end = 8.dp, bottom = 8.dp),
    ) {
        BasicText(q.author, style = TextStyle(fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Label2))
        BasicText(q.excerpt, style = TextStyle(fontSize = 13.sp, color = Label2), maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun PhotoGrid(photos: List<Photo>, width: Dp) {
    val gap = 4.dp
    Box(Modifier.padding(top = 8.dp).clip(RoundedCornerShape(12.dp))) {
        when (photos.size) {
            1 -> { val p = photos[0]; BundleImage(p.src, width, min((width * p.h / p.w).value, 320f).dp) }
            2 -> {
                val s = (width - gap) / 2
                Row(horizontalArrangement = Arrangement.spacedBy(gap)) { photos.forEach { BundleImage(it.src, s, s) } }
            }
            3 -> {
                val small = (width - gap) / 3; val big = width - gap - small; val h = (big - gap) / 2
                Row(horizontalArrangement = Arrangement.spacedBy(gap)) {
                    BundleImage(photos[0].src, big, big)
                    Column(verticalArrangement = Arrangement.spacedBy(gap)) {
                        BundleImage(photos[1].src, small, h)
                        BundleImage(photos[2].src, small, h)
                    }
                }
            }
            else -> {
                val s = (width - gap) / 2
                Column(verticalArrangement = Arrangement.spacedBy(gap)) {
                    for (row in 0..1) Row(horizontalArrangement = Arrangement.spacedBy(gap)) {
                        BundleImage(photos[row * 2].src, s, s); BundleImage(photos[row * 2 + 1].src, s, s)
                    }
                }
            }
        }
    }
}

@Composable
private fun LinkCardView(link: LinkCard, width: Dp) {
    val shape = RoundedCornerShape(12.dp)
    Column(Modifier.padding(top = 8.dp).fillMaxWidth().clip(shape).border(0.5.dp, CardBorder, shape)) {
        BundleImage(link.thumb, width, 140.dp)
        Column(Modifier.padding(10.dp)) {
            BasicText(link.site.uppercase(), style = TextStyle(fontSize = 12.sp, color = Secondary))
            BasicText(link.title, style = TextStyle(fontSize = 15.sp, fontWeight = FontWeight.SemiBold), maxLines = 2, overflow = TextOverflow.Ellipsis)
            BasicText(link.description, style = TextStyle(fontSize = 13.sp, color = Label2), maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** An image box: #E5E5EA until its decode at the box's pixel size arrives. */
@Composable
private fun BundleImage(name: String, w: Dp, h: Dp, modifier: Modifier = Modifier) {
    val density = LocalDensity.current
    val wpx = with(density) { w.roundToPx() }; val hpx = with(density) { h.roundToPx() }
    val key = Images.key(name, wpx, hpx)
    val context = LocalContext.current
    var bitmap by remember(key) { mutableStateOf<Bitmap?>(Images.cached(key)) }
    if (bitmap == null) LaunchedEffect(key) {
        bitmap = suspendCancellableCoroutine { c ->
            val f = Images.load(context, name, wpx, hpx) { c.resumeWith(Result.success(it)) }
            c.invokeOnCancellation { f.cancel(true) }
        }
    }
    Box(modifier.size(w, h).background(Hairline)) {
        bitmap?.let { Image(it.asImageBitmap(), null, Modifier.size(w, h), contentScale = ContentScale.Crop) }
    }
}
