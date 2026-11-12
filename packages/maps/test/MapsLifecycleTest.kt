package dev.exact.canvas

import android.content.Context
import android.os.Bundle
import android.view.View
import com.google.android.gms.maps.*
import com.google.android.gms.maps.model.*
import exact.maps.MapsModule
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadow.api.Shadow
import java.io.File

/** Run the package and hosted lifecycle; replace only the SDK's remote map with a saved camera. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE,
    shadows = [MapsLifecycleTest.Engine::class, MapsLifecycleTest.MapSdk::class, MapsLifecycleTest.MapSurface::class, MapsLifecycleTest.Updates::class],
    instrumentedPackages = ["com.google.android.gms.maps", "dev.exact.canvas.Native"])
class MapsLifecycleTest {
    @Implements(Native::class)
    class Engine {
        companion object {
            @Implementation @JvmStatic fun __staticInitializer__() {}
            @Implementation @JvmStatic fun underAgent(name: String) = false
        }
    }

    @Implements(GoogleMap::class)
    class MapSdk {
        var camera = CameraPosition(LatLng(0.0, 0.0), 0f, 0f, 0f)
        var moves = 0
        @Implementation fun getCameraPosition() = camera
        @Implementation fun moveCamera(update: CameraUpdate) {
            camera = Updates.changes.getValue(update)(camera); moves++
        }
        @Implementation fun animateCamera(update: CameraUpdate) = moveCamera(update)
        @Implementation fun setOnMarkerClickListener(listener: GoogleMap.OnMarkerClickListener?) {}
        @Implementation fun setOnMapClickListener(listener: GoogleMap.OnMapClickListener?) {}
        @Implementation fun setOnCameraIdleListener(listener: GoogleMap.OnCameraIdleListener?) {}
    }

    @Implements(CameraUpdateFactory::class)
    class Updates {
        companion object {
            val changes = java.util.IdentityHashMap<CameraUpdate, (CameraPosition) -> CameraPosition>()
            private fun update(change: (CameraPosition) -> CameraPosition) = Shadow.newInstanceOf(CameraUpdate::class.java).also { changes[it] = change }
            @Implementation @JvmStatic fun newLatLngZoom(p: LatLng, z: Float) = update { CameraPosition(p, z, it.tilt, it.bearing) }
            @Implementation @JvmStatic fun newCameraPosition(p: CameraPosition) = update { p }
            @Implementation @JvmStatic fun newLatLng(p: LatLng) = update { CameraPosition(p, it.zoom, it.tilt, it.bearing) }
            @Implementation @JvmStatic fun zoomTo(z: Float) = update { CameraPosition(it.target, z, it.tilt, it.bearing) }
        }
    }

    @Implements(MapView::class)
    class MapSurface : org.robolectric.shadows.ShadowViewGroup() {
        val map = Shadow.newInstanceOf(GoogleMap::class.java)
        val sdk: MapSdk get() = Shadow.extract(map)
        @Implementation fun onCreate(state: Bundle?) {
            state?.getParcelable<CameraPosition>("camera")?.let { sdk.camera = it }
        }
        @Implementation fun getMapAsync(callback: OnMapReadyCallback) { callback.onMapReady(map) }
        @Implementation fun onSaveInstanceState(state: Bundle) { state.putParcelable("camera", sdk.camera) }
        @Implementation fun onStart() {}
        @Implementation fun onResume() {}
        @Implementation fun onPause() {}
        @Implementation fun onStop() {}
        @Implementation fun onDestroy() {}
    }

    private class Fixture {
        val controller = Robolectric.buildActivity(android.app.Activity::class.java).setup()
        val activity = controller.get()
        val module: MapsModule
        var current: MapSurface? = null
        init {
            activity.applicationInfo.metaData = Bundle().apply { putString("com.google.android.geo.API_KEY", "test-not-a-key") }
            module = MapsModule(ExtensionContext(activity, "maps"))
        }
        val adapter = object : NativeModule {
            override fun create(context: Context, props: JSONObject): View =
                module.view("exact-map", "1", "map", context, props) { _, _ -> }.also { current = Shadow.extract(it) }
            override fun update(view: View, props: JSONObject) = module.update(view, props)
            override fun lifecycle(view: View, event: ViewLifecycle, state: Bundle?) = module.viewLifecycle(view, event, state)
            override fun destroy(view: View) = module.destroyView(view)
        }
        fun host(state: Bundle? = null) = Natives(activity, File("."), mapOf("map" to adapter)).also {
            it.lifecycle.restore(state)
            it.lifecycle.event(ViewLifecycle.START); it.lifecycle.event(ViewLifecycle.RESUME)
        }
        fun show(host: Natives, zoom: Number? = null, latitude: Double = 1.0, longitude: Double = 2.0, decimalJson: Boolean = false) {
            val props = JSONObject().put("latitude", latitude).put("longitude", longitude)
            zoom?.let { props.put("zoomLevel", it) }
            val json = if (decimalJson) "{\"latitude\":$latitude,\"longitude\":$longitude,\"zoomLevel\":$zoom}" else props.toString()
            val row = JSONObject().put("module", "map").put("slot", 2).put("generation", 3).put("props", json)
            host.ensure(7, 2, 200, 300, row.toString(), 0)
        }
        fun finish(host: Natives) { host.destroyAll(); controller.pause().stop().destroy() }
    }

    @Test fun aMapReservesItsPanBeforeTheScrollerCanIntercept() {
        val f = Fixture()
        val host = f.host()
        f.show(host)
        val map = host.taker(7, 2, 3)!!
        map.setOnTouchListener { _, _ -> true }
        val down = android.view.MotionEvent.obtain(100, 100, 0, 50f, 50f, 0)
        assertTrue(map.dispatchTouchEvent(down))
        down.recycle()
        assertTrue(host.keeps)
        f.finish(host)
    }

    @Test fun backgroundAndReturnPreserveThePersonsCamera() {
        val f = Fixture()
        val host = f.host()
        f.show(host)
        assertEquals(12f, f.current!!.sdk.camera.zoom)
        assertEquals(LatLng(1.0, 2.0), f.current!!.sdk.camera.target)
        val panned = CameraPosition(LatLng(40.0, 50.0), 17f, 30f, 45f)
        f.current!!.sdk.camera = panned
        host.lifecycle.event(ViewLifecycle.PAUSE); host.lifecycle.event(ViewLifecycle.STOP)
        val state = host.lifecycle.save()
        host.destroyAll()
        val next = f.host(state)
        f.show(next)
        assertEquals(panned, f.current!!.sdk.camera)
        assertEquals(0, f.current!!.sdk.moves)
        f.finish(next)
    }

    @Test fun activityRecreationPreservesTheCameraAndAppliesOnlyChangedProps() {
        for (change in listOf("none", "zoom", "latitude", "longitude", "both", "centre+zoom")) {
            val f = Fixture()
            val host = f.host()
            f.show(host, zoom = 12)
            val panned = CameraPosition(LatLng(40.0, 50.0), 17f, 30f, 45f)
            f.current!!.sdk.camera = panned
            val state = host.lifecycle.save()
            // A fresh activity and module must recover the comparison from the bundle, not live memory.
            f.finish(host)
            val next = Fixture()
            val restored = next.host(state)
            next.show(restored, zoom = if (change in listOf("zoom", "centre+zoom")) 14 else 12,
                latitude = if (change in listOf("latitude", "both", "centre+zoom")) 3.0 else 1.0,
                longitude = if (change in listOf("longitude", "both", "centre+zoom")) 4.0 else 2.0)
            val camera = next.current!!.sdk.camera
            assertEquals(LatLng(if (change in listOf("latitude", "both", "centre+zoom")) 3.0 else 40.0,
                if (change in listOf("longitude", "both", "centre+zoom")) 4.0 else 50.0), camera.target)
            assertEquals(if (change in listOf("zoom", "centre+zoom")) 14f else panned.zoom, camera.zoom)
            assertEquals(panned.tilt, camera.tilt)
            assertEquals(panned.bearing, camera.bearing)
            assertEquals(if (change == "none") 0 else 1, next.current!!.sdk.moves)
            next.finish(restored)
        }
    }

    @Test fun integerCameraValuesRestoredFromJsonDoNotMoveTheMap() {
        val f = Fixture()
        val host = f.host()
        f.show(host, zoom = 12.0)
        val panned = CameraPosition(LatLng(40.0, 50.0), 17f, 30f, 45f)
        f.current!!.sdk.camera = panned
        val state = host.lifecycle.save()
        // Android's JSON writer omits the decimal part of whole numbers; the JVM JSON library need not.
        state.getBundle(state.keySet().single())!!.putString("exact.maps.cameraProps",
            "{\"latitude\":1,\"longitude\":2,\"zoomLevel\":12}")
        f.finish(host)
        val next = Fixture()
        val restored = next.host(state)
        next.show(restored, zoom = 12.0, decimalJson = true)
        assertEquals(panned, next.current!!.sdk.camera)
        assertEquals(0, next.current!!.sdk.moves)
        next.finish(restored)
    }
}
