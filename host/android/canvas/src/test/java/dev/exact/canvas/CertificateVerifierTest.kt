package dev.exact.canvas

import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.res.AssetManager
import android.content.res.Resources
import android.content.Intent
import android.content.res.XmlResourceParser
import android.net.http.X509TrustManagerExtensions
import android.os.Looper
import android.security.KeyChain
import android.util.Xml
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.io.StringReader
import java.lang.reflect.InvocationTargetException
import java.lang.reflect.Modifier
import java.lang.reflect.Proxy
import java.security.Key
import java.security.KeyStore
import java.security.KeyStoreSpi
import java.security.Provider
import java.security.Security
import java.security.cert.Certificate
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import java.util.Collections
import java.util.Date
import javax.net.ssl.ManagerFactoryParameters
import javax.net.ssl.TrustManager
import javax.net.ssl.TrustManagerFactorySpi
import javax.net.ssl.X509TrustManager
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowArscAssetManager10
import org.robolectric.shadows.ShadowApplicationPackageManager

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE,
    shadows = [CertificateVerifierTest.Extensions::class, CertificateVerifierTest.Assets::class,
        CertificateVerifierTest.Packages::class])
class CertificateVerifierTest {
    companion object {
        private var manifest = ""
        private var manifestUnavailable = false
        // The phone's store; a device trust manager copies it when made, as Android's KeyStoreCertificateSource does.
        private val deviceAnchors = mutableSetOf<String>()
    }

    @Implements(className = "android.app.ApplicationPackageManager")
    class Packages : ShadowApplicationPackageManager() {
        @Implementation override fun getResourcesForApplication(info: ApplicationInfo): Resources {
            assertNull("only the base APK may supply the manifest", info.splitSourceDirs)
            assertNull(info.splitPublicSourceDirs)
            assertNull(info.splitNames)
            return super.getResourcesForApplication(info)
        }
    }

    @Implements(AssetManager::class)
    class Assets : ShadowArscAssetManager10() {
        @Implementation fun openXmlResourceParser(file: String): XmlResourceParser {
            assertEquals("AndroidManifest.xml", file)
            if (manifestUnavailable) throw IOException("unreadable manifest")
            val parser = Xml.newPullParser().apply { setInput(StringReader(manifest)) }
            return Proxy.newProxyInstance(XmlResourceParser::class.java.classLoader,
                arrayOf(XmlResourceParser::class.java)) { _, method, args ->
                when (method.name) {
                    "close" -> null
                    "getAttributeResourceValue" -> if (parser.getAttributeValue(args!![0] as String,
                        args[1] as String)?.let { it != "@null" } == true) 1 else 0
                    else -> try { method.invoke(parser, *(args ?: emptyArray())) }
                        catch (error: InvocationTargetException) { throw error.cause!! }
                }
            } as XmlResourceParser
        }
    }

    // Model Android's factory: null selects app policy; an explicit store bypasses it.
    class Factory : TrustManagerFactorySpi() {
        private var device: Set<String>? = null
        override fun engineInit(store: KeyStore?) { device = store?.let { deviceAnchors.toSet() } }
        override fun engineInit(parameters: ManagerFactoryParameters?) = error("unexpected parameters")
        override fun engineGetTrustManagers(): Array<TrustManager> = arrayOf(Policy(device))
    }

    class Policy(private val device: Set<String>?) : X509TrustManager {
        override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
        override fun checkClientTrusted(chain: Array<X509Certificate>, auth: String) = error("client")
        override fun checkServerTrusted(chain: Array<X509Certificate>, auth: String) = error("hostname required")
        fun check(chain: Array<X509Certificate>, auth: String, host: String): List<X509Certificate> {
            assertEquals("RSA", auth)
            val trusted = device?.contains(host) ?: (host in setOf("private.test", "allowed-user.test"))
            if (!trusted) throw CertificateException("CA excluded by policy for $host")
            return chain.toList()
        }
    }

    class Store : KeyStoreSpi() {
        override fun engineLoad(stream: InputStream?, password: CharArray?) {}
        override fun engineStore(stream: OutputStream?, password: CharArray?) = error("read only")
        override fun engineGetKey(alias: String?, password: CharArray?): Key? = null
        override fun engineGetCertificateChain(alias: String?): Array<Certificate>? = null
        override fun engineGetCertificate(alias: String?): Certificate? = null
        override fun engineGetCreationDate(alias: String?): Date? = null
        override fun engineSetKeyEntry(alias: String?, key: Key?, password: CharArray?, chain: Array<Certificate>?) = error("read only")
        override fun engineSetKeyEntry(alias: String?, key: ByteArray?, chain: Array<Certificate>?) = error("read only")
        override fun engineSetCertificateEntry(alias: String?, cert: Certificate?) = error("read only")
        override fun engineDeleteEntry(alias: String?) = error("read only")
        override fun engineAliases() = Collections.emptyEnumeration<String>()
        override fun engineContainsAlias(alias: String?) = false
        override fun engineSize() = 0
        override fun engineIsKeyEntry(alias: String?) = false
        override fun engineIsCertificateEntry(alias: String?) = false
        override fun engineGetCertificateAlias(cert: Certificate?): String? = null
    }

    @Suppress("DEPRECATION")
    class TestProvider : Provider("ExactTlsTest", 1.0, "Android trust policy fixture") {
        init {
            put("TrustManagerFactory.ExactPolicy", Factory::class.java.name)
            put("KeyStore.AndroidCAStore", Store::class.java.name)
        }
    }

    @Implements(X509TrustManagerExtensions::class)
    class Extensions {
        private lateinit var manager: Policy
        @Implementation fun __constructor__(manager: X509TrustManager) { this.manager = manager as Policy }
        @Implementation fun checkServerTrusted(chain: Array<X509Certificate>, auth: String, host: String) =
            manager.check(chain, auth, host)
    }

    private val algorithm = Security.getProperty("ssl.TrustManagerFactory.algorithm")
    @Before fun setup() {
        manifest = """<manifest xmlns:android="http://schemas.android.com/apk/res/android">
            <application android:networkSecurityConfig="@xml/policy" /></manifest>"""
        manifestUnavailable = false
        deviceAnchors.clear()
        deviceAnchors += listOf("system.test", "user.test", "allowed-user.test", "excluded-user.test")
        Security.insertProviderAt(TestProvider(), 1)
        Security.setProperty("ssl.TrustManagerFactory.algorithm", "ExactPolicy")
    }
    @After fun cleanup() {
        Security.setProperty("ssl.TrustManagerFactory.algorithm", algorithm)
        Security.removeProvider("ExactTlsTest")
    }

    private val leaf get() = File("../../../vendor/ibex/crates/ibex2/src/transport/testdata/distrusted/trusted-leaf.der").readBytes()

    @Test fun theCompanionImplementsTheVerifiersJniInterface() {
        val verifier = Class.forName("org.rustls.platformverifier.CertificateVerifier")
        val result = Class.forName("org.rustls.platformverifier.VerificationResult")
        val method = verifier.getDeclaredMethod("verifyCertificateChain", Context::class.java,
            String::class.java, String::class.java, Array<String>::class.java, ByteArray::class.java,
            Long::class.javaPrimitiveType, Array<ByteArray>::class.java)
        assertTrue(Modifier.isStatic(method.modifiers))
        assertEquals(result, method.returnType)
        assertEquals(Int::class.javaPrimitiveType, result.getDeclaredField("code").type)
        assertEquals(String::class.java, result.getDeclaredField("message").type)
        val status = Class.forName("org.rustls.platformverifier.Status")
        val code = status.getDeclaredField("code").apply { isAccessible = true }
        assertEquals(mapOf("Ok" to 0, "Unavailable" to 1, "Expired" to 2, "UnknownCert" to 3,
            "Revoked" to 4, "InvalidEncoding" to 5, "InvalidExtension" to 6),
            status.enumConstants!!.associate { (it as Enum<*>).name to code.getInt(it) })
    }

    private fun verify(host: String, certificates: Array<ByteArray> = arrayOf(leaf),
        usages: Array<String> = arrayOf("1.3.6.1.5.5.7.3.1"), now: Long = 1_794_633_663_000L): Int {
        val method = Class.forName("org.rustls.platformverifier.CertificateVerifier").getDeclaredMethod("verifyCertificateChain",
            Context::class.java, String::class.java, String::class.java, Array<String>::class.java,
            ByteArray::class.java, Long::class.javaPrimitiveType, Array<ByteArray>::class.java)
        method.isAccessible = true
        val result = method.invoke(null, RuntimeEnvironment.getApplication(), host, "RSA",
            usages, null, now, certificates)
        return result.javaClass.getDeclaredField("code").apply { isAccessible = true }.getInt(result)
    }

    @Test fun appAnchorsAreAcceptedAndExcludedStoreAnchorsAreRejectedForTheRequestedHost() {
        assertEquals("an app-only CA must be accepted", 0, verify("private.test"))
        assertEquals("a user CA admitted by the app must be accepted", 0, verify("allowed-user.test"))
        assertEquals("a user CA excluded by the app must be rejected", 3, verify("excluded-user.test"))
        assertEquals("a domain-specific CA must not escape its domain", 3, verify("other.test"))
    }

    @Test fun anAppWithoutTrustPolicyAcceptsThePhonesSystemAndUserAnchors() {
        for (attribute in listOf("", "android:networkSecurityConfig=\"@null\"")) {
            manifest = """<manifest xmlns:android="http://schemas.android.com/apk/res/android">
                <application $attribute /></manifest>"""
            assertEquals("the phone's user anchors must be accepted", 0, verify("user.test"))
            assertEquals("the phone's system anchors must be accepted", 0, verify("system.test"))
            assertEquals("an unknown CA must still be rejected", 3, verify("other.test"))
        }
    }

    @Test fun aCaInstalledOrRemovedAfterAVerificationIsSeenOnceAndroidReportsTheChange() {
        manifest = """<manifest xmlns:android="http://schemas.android.com/apk/res/android"><application /></manifest>"""
        assertEquals(0, verify("user.test"))
        assertEquals(3, verify("installed.test"))
        deviceAnchors += "installed.test"
        deviceAnchors -= "user.test"
        RuntimeEnvironment.getApplication().sendBroadcast(Intent(KeyChain.ACTION_TRUST_STORE_CHANGED))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("a CA installed after the first verification must be accepted", 0, verify("installed.test"))
        assertEquals("a CA removed after the first verification must be rejected", 3, verify("user.test"))
        assertEquals(0, verify("system.test"))
    }

    @Test fun anUnreadablePolicyDoesNotFallBackToThePhonesStore() {
        manifestUnavailable = true
        assertEquals(1, verify("user.test"))
        manifestUnavailable = false
        manifest = "<manifest><application"
        assertEquals(1, verify("user.test"))
        manifest = "<manifest />"
        assertEquals(1, verify("user.test"))
    }

    @Test fun splitManifestsCannotHideTheBaseApksPolicy() {
        val info = RuntimeEnvironment.getApplication().applicationInfo
        info.splitSourceDirs = arrayOf("split.apk")
        info.splitPublicSourceDirs = arrayOf("split.apk")
        info.splitNames = arrayOf("split")
        assertEquals(0, verify("private.test"))
        assertEquals(3, verify("excluded-user.test"))
        assertArrayEquals(arrayOf("split.apk"), info.splitSourceDirs)
        assertArrayEquals(arrayOf("split.apk"), info.splitPublicSourceDirs)
        assertArrayEquals(arrayOf("split"), info.splitNames)
    }

    @Test fun invalidCertificatesAreRejectedBeforeConsultingTrustPolicy() {
        assertEquals(5, verify("private.test", certificates = emptyArray()))
        assertEquals(5, verify("private.test", certificates = arrayOf(byteArrayOf(1, 2, 3))))
        assertEquals(6, verify("private.test", usages = emptyArray()))
        assertEquals(2, verify("private.test", now = 9_000_000_000_000L))
        assertEquals(2, verify("private.test", now = 0))
    }
}
