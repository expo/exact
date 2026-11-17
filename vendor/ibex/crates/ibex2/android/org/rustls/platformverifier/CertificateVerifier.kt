package org.rustls.platformverifier

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.net.http.X509TrustManagerExtensions
import android.os.Build
import android.security.KeyChain
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.cert.CertificateException
import java.security.cert.CertificateExpiredException
import java.security.cert.CertificateNotYetValidException
import java.security.cert.CertificateRevokedException
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.util.Date
import javax.net.ssl.TrustManagerFactory
import javax.net.ssl.X509TrustManager
import org.xmlpull.v1.XmlPullParser
import org.xmlpull.v1.XmlPullParserException

// JNI companion for rustls-platform-verifier 0.7 (Android artifact 0.2.0).
// 0.7.1 keeps the 0.7.0 interface; CertificateVerifierTest checks the method and result fields.
internal object CertificateVerifier {
    private fun manager(store: KeyStore?): X509TrustManagerExtensions {
        val factory = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm())
        factory.init(store)
        return X509TrustManagerExtensions(factory.trustManagers.filterIsInstance<X509TrustManager>().single())
    }

    // A null store honors app and domain policy, and Android refreshes it when the trust store changes.
    private val appTrustManager by lazy { manager(null) }

    // AndroidCAStore also includes user-installed CAs, but a trust manager made over it copies the store's
    // anchors once (KeyStoreCertificateSource), so it is made again after Android reports a change.
    private var watched: Context? = null
    @Volatile private var changes = 0
    private var device: Pair<Int, X509TrustManagerExtensions>? = null

    @Synchronized private fun deviceTrustManager(context: Context): X509TrustManagerExtensions {
        val application = context.applicationContext
        if (watched !== application) {
            val receiver = object : BroadcastReceiver() {
                override fun onReceive(context: Context, intent: Intent) { changes++ }
            }
            val filter = IntentFilter(KeyChain.ACTION_TRUST_STORE_CHANGED)
            // The system sends this broadcast, which reaches a receiver not exported to other apps.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                application.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
            } else {
                application.registerReceiver(receiver, filter)
            }
            watched = application
            device = null
        }
        // Taken before the store is read: a change reported while it is read makes the next verification build again.
        val seen = changes
        device?.let { (made, manager) -> if (made == seen) return manager }
        return manager(KeyStore.getInstance("AndroidCAStore").apply { load(null) }).also { device = seen to it }
    }

    private fun usesAppPolicy(context: Context): Boolean {
        try {
            // Read the base APK: a split manifest must not hide the application's policy.
            val base = ApplicationInfo(context.applicationInfo).apply {
                splitSourceDirs = null
                splitPublicSourceDirs = null
                splitNames = null
            }
            val resources = context.packageManager.getResourcesForApplication(base)
            resources.assets.openXmlResourceParser("AndroidManifest.xml").use { xml ->
                while (xml.next() != XmlPullParser.END_DOCUMENT) {
                    if (xml.eventType == XmlPullParser.START_TAG && xml.depth == 2 && xml.name == "application") {
                        return xml.getAttributeResourceValue("http://schemas.android.com/apk/res/android",
                            "networkSecurityConfig", 0) != 0
                    }
                }
            }
        } catch (error: IOException) {
            throw GeneralSecurityException("Cannot read application trust policy", error)
        } catch (error: XmlPullParserException) {
            throw GeneralSecurityException("Cannot parse application trust policy", error)
        } catch (error: PackageManager.NameNotFoundException) {
            throw GeneralSecurityException("Cannot find application trust policy", error)
        }
        throw GeneralSecurityException("Application manifest has no application")
    }

    @Suppress("UNUSED_PARAMETER")
    @JvmStatic fun verifyCertificateChain(
        context: Context,
        serverName: String,
        authType: String,
        allowedEkus: Array<String>,
        ocspResponse: ByteArray?,
        now: Long,
        certificates: Array<ByteArray>,
    ): VerificationResult {
        if (certificates.isEmpty()) return VerificationResult(Status.InvalidEncoding)
        val chain = try {
            val factory = CertificateFactory.getInstance("X.509")
            certificates.map { factory.generateCertificate(it.inputStream()) as X509Certificate }.toTypedArray()
        } catch (error: CertificateException) {
            return VerificationResult(Status.InvalidEncoding, error.message)
        }
        return try {
            chain[0].checkValidity(Date(now))
            val usages = chain[0].extendedKeyUsage
            if (usages != null && usages.none { it in allowedEkus }) {
                return VerificationResult(Status.InvalidExtension)
            }
            val manager = if (usesAppPolicy(context)) appTrustManager else deviceTrustManager(context)
            manager.checkServerTrusted(chain, authType, serverName)
            // rustls-platform-verifier checks the certificate's hostname after this returns.
            VerificationResult(Status.Ok)
        } catch (error: CertificateExpiredException) {
            VerificationResult(Status.Expired, error.toString())
        } catch (error: CertificateNotYetValidException) {
            VerificationResult(Status.Expired, error.toString())
        } catch (error: CertificateRevokedException) {
            VerificationResult(Status.Revoked, error.toString())
        } catch (error: CertificateException) {
            VerificationResult(Status.UnknownCert, error.toString())
        } catch (error: GeneralSecurityException) {
            VerificationResult(Status.Unavailable, error.toString())
        }
    }
}

// These codes and fields are read by rustls-platform-verifier's Android JNI.
internal enum class Status(val code: Int) {
    Ok(0), Unavailable(1), Expired(2), UnknownCert(3), Revoked(4), InvalidEncoding(5), InvalidExtension(6),
}

internal class VerificationResult(status: Status, @JvmField val message: String? = null) {
    @JvmField val code = status.code
}
