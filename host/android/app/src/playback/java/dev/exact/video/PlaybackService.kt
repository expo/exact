package dev.exact.video

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.drawable.Icon
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.ui.R as MediaResources
import dev.exact.canvas.ForegroundServices

/** The service takes the audible player as soon as playback starts, including while Android starts the service. */
@OptIn(UnstableApi::class)
class PlaybackService : MediaSessionService() {
    private val notifications get() = getSystemService(NotificationManager::class.java)
    private val appName by lazy { applicationInfo.loadLabel(packageManager).toString() }
    private var session: MediaSession? = null
    private var promoted = false

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            START -> {
                starting = false
                service = this
                refresh()
            }
            PLAY -> session?.player?.play()
            PAUSE -> session?.player?.pause()
        }
        if (!promoted) stopSelf(startId)
        return START_NOT_STICKY
    }

    private fun refresh() {
        val player = (claims.lastOrNull { it.player.playWhenReady } ?: claims.lastOrNull())?.player
        if (session != null && player != null) {
            if (session?.player !== player) session?.player = player
            return
        }
        if (player == null && promoted) { finish(); return }
        // A pause can arrive before onStartCommand. Satisfy Android's foreground deadline even for that start.
        val made = player?.let {
            val builder = MediaSession.Builder(this, it)
            packageManager.getLaunchIntentForPackage(packageName)?.let { launch ->
                builder.setSessionActivity(PendingIntent.getActivity(this, 0, launch, PendingIntent.FLAG_IMMUTABLE))
            }
            builder.build()
        }
        session = made
        try {
            notifications.createNotificationChannel(NotificationChannel(CHANNEL, appName, NotificationManager.IMPORTANCE_LOW))
            startForeground(NOTIFICATION, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } catch (e: IllegalStateException) {
            refused(e)
            return
        } catch (e: SecurityException) {
            refused(e)
            return
        }
        if (made == null) { finish(); return }
        promoted = true
        addSession(made)
        ForegroundServices.playbackPromoted(true)
    }

    // Publish one notification for the service's whole lifetime, including when its session changes players.
    override fun onUpdateNotification(session: MediaSession, startInForegroundRequired: Boolean) {
        if (promoted && session === this.session) notifications.notify(NOTIFICATION, notification())
    }

    private fun notification(): Notification {
        val current = session
        val builder = Notification.Builder(this, CHANNEL)
            .setSmallIcon(MediaResources.drawable.exo_notification_small_icon)
            .setContentTitle(current?.player?.mediaMetadata?.title ?: appName)
            .setCategory(Notification.CATEGORY_TRANSPORT)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setShowWhen(false)
            .setOnlyAlertOnce(true)
            .setOngoing(current?.player?.playWhenReady == true)
        if (current != null) {
            val playing = current.player.playWhenReady
            val icon = if (playing) MediaResources.drawable.exo_notification_pause else MediaResources.drawable.exo_notification_play
            val label = if (playing) MediaResources.string.exo_controls_pause_description else MediaResources.string.exo_controls_play_description
            val action = PendingIntent.getService(this, 0, Intent(this, PlaybackService::class.java).setAction(if (playing) PAUSE else PLAY),
                PendingIntent.FLAG_IMMUTABLE)
            builder.setContentIntent(current.sessionActivity)
                .addAction(Notification.Action.Builder(Icon.createWithResource(this, icon), getText(label), action).build())
                .setStyle(Notification.MediaStyle().setMediaSession(current.platformToken).setShowActionsInCompactView(0))
        }
        return builder.build()
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        if (claims.none { it.player.playWhenReady }) stopSelf()
    }

    private fun refused(error: Exception) {
        Log.w("exact", "Android refused the media playback service", error)
        finish()
        stopPlayers()
    }

    private fun finish() {
        forgetSession()
        if (service === this) {
            service = null
            ForegroundServices.playbackPromoted(false)
        }
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun forgetSession() {
        promoted = false
        val previous = session ?: return
        session = null
        if (isSessionAdded(previous)) removeSession(previous)
        previous.release()
    }

    override fun onDestroy() {
        forgetSession()
        if (service === this) {
            service = null
            ForegroundServices.playbackPromoted(false)
            stopPlayers()
        }
        super.onDestroy()
    }

    internal companion object {
        private const val START = "dev.exact.video.START_PLAYBACK"
        private const val PLAY = "dev.exact.video.PLAY_PLAYBACK"
        private const val PAUSE = "dev.exact.video.PAUSE_PLAYBACK"
        private const val CHANNEL = "exact.playback"
        private const val NOTIFICATION = 0x45584303
        private val claims = LinkedHashSet<Playback>()
        private var service: PlaybackService? = null
        private var starting = false

        fun claim(context: Context, player: Player, lost: () -> Unit): Playback? {
            val claim = Playback(player, lost)
            claims.add(claim)
            val live = service
            if (live == null && !starting) {
                try {
                    context.startForegroundService(Intent(context, PlaybackService::class.java).setAction(START))
                    starting = true
                } catch (e: IllegalStateException) {
                    Log.w("exact", "Android refused to start background playback", e)
                    claims.remove(claim)
                    return null
                } catch (e: SecurityException) {
                    Log.w("exact", "Android refused to start background playback", e)
                    claims.remove(claim)
                    return null
                }
            }
            player.addListener(claim)
            live?.refresh()
            return claim
        }

        fun playbackChanged(claim: Playback) {
            if (claim.player.playWhenReady && claims.remove(claim)) claims.add(claim)
            service?.refresh()
        }

        fun end(claim: Playback) {
            claim.player.removeListener(claim)
            if (!claims.remove(claim)) return
            // Switch the session before the reader can release this player.
            service?.refresh()
        }

        private fun stopPlayers() {
            val lost = claims.toList()
            claims.clear()
            for (claim in lost) claim.lost()
        }
    }
}

internal class Playback(val player: Player, val lost: () -> Unit) : AutoCloseable, Player.Listener {
    override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) = PlaybackService.playbackChanged(this)
    override fun close() = PlaybackService.end(this)
}
