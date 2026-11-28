package dev.exact.app

import android.content.Context
import dev.exact.canvas.VideoPlayer
import dev.exact.video.MediaPlayer
import dev.exact.video.PlaybackService

/** Media3's player with a system media session for audible audio. */
internal val videoPlayers: ((Context) -> VideoPlayer)? = { context ->
    val application = context.applicationContext
    MediaPlayer(context) { player, lost -> PlaybackService.claim(application, player, lost) }
}
