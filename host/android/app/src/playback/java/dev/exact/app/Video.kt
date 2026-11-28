package dev.exact.app

import android.content.Context
import android.graphics.Bitmap
import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.MediaMetadata
import androidx.media3.common.util.BitmapLoader
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.session.CacheBitmapLoader
import com.google.common.util.concurrent.ListenableFuture
import dev.exact.canvas.VideoPlayer
import dev.exact.video.MediaPlayer
import dev.exact.video.PlaybackService

/** Media3's player with a system media session for audible audio. */
@OptIn(UnstableApi::class)
internal val videoPlayers: ((Context) -> VideoPlayer)? = { context ->
    val application = context.applicationContext
    val artwork = PlaybackArtwork(CacheBitmapLoader(DataSourceBitmapLoader.Builder(application).setMaximumOutputDimension(512).build()))
    MediaPlayer(context, artwork) { player -> PlaybackService.claim(application, player, artwork) }
}

// Retain the item's request while the session reads other images, such as embedded stream artwork.
@OptIn(UnstableApi::class)
private class PlaybackArtwork(private val decoder: BitmapLoader) : BitmapLoader by decoder {
    private var source: Uri? = null
    private var pending: ListenableFuture<Bitmap>? = null

    override fun loadBitmap(uri: Uri): ListenableFuture<Bitmap> {
        if (uri == source) pending?.let { return it }
        source = uri
        return decoder.loadBitmap(uri).also { pending = it }
    }

    override fun loadBitmapFromMetadata(metadata: MediaMetadata): ListenableFuture<Bitmap>? {
        if (source != null && metadata.artworkUri == source) return pending
        return decoder.loadBitmapFromMetadata(metadata)
    }
}
