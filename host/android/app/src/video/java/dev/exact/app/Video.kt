package dev.exact.app

import android.content.Context
import dev.exact.canvas.VideoPlayer
import dev.exact.video.MediaPlayer

/** Media3's player for the plan's `video` and `audio` elements (LLP 1042.001). */
internal val videoPlayers: ((Context) -> VideoPlayer)? = { context -> MediaPlayer(context) }
