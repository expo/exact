package dev.exact.app

import android.content.Context
import dev.exact.canvas.VideoPlayer

/** A production APK whose plan has no `video` or `audio` links no player (LLP 1042.001 D2). */
internal val videoPlayers: ((Context) -> VideoPlayer)? = null
