package dev.exact.car

import android.content.pm.PackageManager
import android.os.Binder
import android.os.IBinder
import android.os.Parcel
import android.util.Log
import androidx.car.app.CarAppService
import androidx.car.app.HostInfo
import androidx.car.app.validation.HostValidator

/** A package supplies onCreateSession after its client passes host validation. */
abstract class PoiCarAppService : CarAppService() {
    // The signer/package pairs in androidx.car.app:app:1.7.0's hosts_allowlist_sample.
    // A dependency can overlay an Android resource at any device configuration.
    final override fun createHostValidator(): HostValidator = HostValidator.Builder(this)
        .addAllowedHost("com.google.android.projection.gearhead", "fdb00c43dbde8b51cb312aa81d3b5fa17713adb94b28f598d77f8eb89daceedf")
        .addAllowedHost("com.google.android.projection.gearhead", "70811a3eacfd2e83e18da9bfede52df16ce91f2e69a44d21f18ab66991130771")
        .addAllowedHost("com.google.android.projection.gearhead", "1975b2f17177bc89a5dff31f9e64a6cae281a53dc1d1d59b1d147fe1c82afa00")
        .addAllowedHost("com.google.android.apps.automotive.templates.host", "c241ffbc8e287c4e9a4ad19632ba1b1351ad361d5177b7d7b29859bd2b7fc631")
        .addAllowedHost("com.google.android.apps.automotive.templates.host", "dd66deaf312d8daec7adbe85a218ecc8c64f3b152f9b5998d5b29300c2623f61")
        .addAllowedHost("com.google.android.apps.automotive.templates.host", "50e603d333c6049a37bd751375d08f3bd0abebd33facd30bd17b64b89658b421")
        .build()
}

// Android calls onBind on the main thread. The identity of the client is available on the
// binder thread for each transaction, including transactions on a previously obtained binder.
fun validatedBinder(delegate: IBinder, service: CarAppService): IBinder {
    if (service !is PoiCarAppService) return delegate
    val validator = service.createHostValidator()
    val packages = service.packageManager
    val automotive = packages.hasSystemFeature(PackageManager.FEATURE_AUTOMOTIVE)
    return object : Binder() {
        override fun onTransact(code: Int, data: Parcel, reply: Parcel?, flags: Int): Boolean {
            val uid = getCallingUid()
            val allowed = synchronized(validator) {
                packages.getPackagesForUid(uid).orEmpty().any { name ->
                    // On a phone Android Auto authenticates by signer. TEMPLATE_RENDERER is an Automotive OS privilege.
                    val renderer = automotive && packages.checkPermission(HostValidator.TEMPLATE_RENDERER_PERMISSION, name) == PackageManager.PERMISSION_GRANTED
                    val eligible = renderer ||
                        validator.allowedHosts[name].orEmpty().any { packages.hasSigningCertificate(name, it.hexToByteArray(), PackageManager.CERT_INPUT_SHA256) }
                    eligible && validator.isValidHost(HostInfo(name, uid))
                }
            }
            if (!allowed) {
                Log.w("ExactCarService", "Rejected car service caller UID $uid")
                if (flags and IBinder.FLAG_ONEWAY == 0) reply?.writeException(SecurityException("Caller UID $uid is not a car host"))
                return true
            }
            return delegate.transact(code, data, reply, flags)
        }
    }
}
