package dev.exact.app

import android.content.Context
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.session.CacheBitmapLoader
import dev.exact.canvas.VideoPlayer
import dev.exact.video.MediaPlayer
import dev.exact.video.PlaybackService

/** Media3's player with a system media session for audible audio. */
@OptIn(UnstableApi::class)
internal val videoPlayers: ((Context) -> VideoPlayer)? = { context ->
    val application = context.applicationContext
    val artwork = CacheBitmapLoader(DataSourceBitmapLoader.Builder(application).setMaximumOutputDimension(512).build())
    MediaPlayer(context, artwork) { player -> PlaybackService.claim(application, player, artwork) }
}
