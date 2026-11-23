package dev.exact.heavybench.views

import android.app.Activity
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import dev.exact.heavybench.Colors
import dev.exact.heavybench.Message
import dev.exact.heavybench.liveStep
import dev.exact.heavybench.loadMessages

/**
 * The heavy list (bench/heavy-list/SPEC.md) in Android Views, written as an Android engineer writes a feed: one
 * RecyclerView with a LinearLayoutManager, one row class whose views are made once and reused, no item animator.
 * Launch extra: `live=1` (live mode).
 */
class MainActivity : Activity() {
    private lateinit var messages: ArrayList<Message>
    private lateinit var list: RecyclerView
    private lateinit var adapter: Feed
    private val main = Handler(Looper.getMainLooper())
    private var seconds = 0

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        messages = loadMessages(this)
        val live = intent.getStringExtra("live") == "1"
        val dp = resources.displayMetrics.density
        val root = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setBackgroundColor(Color.WHITE) }
        root.setOnApplyWindowInsetsListener { v, insets ->
            val bars = insets.getInsets(WindowInsets.Type.systemBars())
            v.setPadding(bars.left, bars.top, bars.right, 0)
            list.setPadding(0, 0, 0, bars.bottom)
            insets
        }
        val bar = LinearLayout(this).apply {
            gravity = Gravity.CENTER_VERTICAL
            setPadding((16 * dp).toInt(), (10 * dp).toInt(), (16 * dp).toInt(), (10 * dp).toInt())
            addView(TextView(context).apply {
                text = "Heavy list"; textSize = 17f; setTextColor(Color.BLACK); typeface = Typeface.create(Typeface.DEFAULT, 600, false)
            }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            addView(TextView(context).apply { text = if (live) "Live: on" else "Live: off"; textSize = 13f; setTextColor(Colors.SECONDARY) })
        }
        root.addView(bar)
        root.addView(View(this).apply { setBackgroundColor(Colors.HAIRLINE) }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 1))
        val contentWidth = resources.displayMetrics.widthPixels - (84 * dp).toInt()
        adapter = Feed(messages, contentWidth) { position, reaction ->
            messages[position] = messages[position].bumped(reaction)
            adapter.notifyItemChanged(position)
        }
        list = (layoutInflater.inflate(R.layout.feed, null) as RecyclerView).apply {
            layoutManager = LinearLayoutManager(context)
            adapter = this@MainActivity.adapter
            itemAnimator = null
            clipToPadding = false
            addItemDecoration(Hairlines(dp))
        }
        root.addView(list, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        setContentView(root)
        if (live) main.postDelayed(object : Runnable {
            var k = 0
            override fun run() {
                k++
                val bumped = liveStep(messages, k, seconds)
                adapter.notifyItemInserted(0)
                if (bumped >= 0) adapter.notifyItemChanged(bumped)
                if (k % 4 == 0) {
                    seconds++
                    for (i in 0 until list.childCount) (list.getChildViewHolder(list.getChildAt(i)) as Feed.Holder).time(seconds)
                }
                main.postDelayed(this, 250)
            }
        }, 250)
    }

    override fun onDestroy() { main.removeCallbacksAndMessages(null); super.onDestroy() }

    private inner class Feed(
        val messages: List<Message>,
        val contentWidth: Int,
        val react: (Int, Int) -> Unit,
    ) : RecyclerView.Adapter<Feed.Holder>() {
        inner class Holder(val row: Row) : RecyclerView.ViewHolder(row) {
            fun time(seconds: Int) { if (bindingAdapterPosition >= 0) row.time(messages[bindingAdapterPosition], seconds) }
        }

        override fun getItemCount() = messages.size
        override fun onCreateViewHolder(parent: ViewGroup, type: Int): Holder {
            val holder = Holder(Row(parent.context, contentWidth))
            holder.row.react = { i -> holder.bindingAdapterPosition.let { if (it >= 0) react(it, i) } }
            return holder
        }
        override fun onBindViewHolder(holder: Holder, position: Int) = holder.row.bind(messages[position], seconds)
    }

    /** The 0.5 pt #E5E5EA hairline under each row, from 68 to width − 16, as the UIKit app draws it. */
    private class Hairlines(private val dp: Float) : RecyclerView.ItemDecoration() {
        private val paint = Paint().apply { color = Colors.HAIRLINE }
        private val height = maxOf(1f, 0.5f * dp)
        override fun onDraw(c: Canvas, parent: RecyclerView, state: RecyclerView.State) {
            for (i in 0 until parent.childCount) {
                val v = parent.getChildAt(i)
                val y = v.bottom + v.translationY
                c.drawRect(68 * dp, y - height, parent.width - 16 * dp, y, paint)
            }
        }
    }
}
