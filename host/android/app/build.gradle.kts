// The app shell: one activity over the reader, packaging the app `-Papp=<name>` names (apps/<name>, or the directory
// `-PappDir=` gives). The package id and the label are `-PapplicationId` and `-PappName`, which agent-android.mjs resolves
// from the manifest (app.json `host.android.id`, default `app.id`; `app.name`), as a selected variant has them (LLP 1030.008).
// `bun scripts/agent-android.mjs build <name> --apk` builds the app's library (apps/<name>/android), puts it and the
// app's assets under this module's build directory, and runs `:app:assembleRelease`.
import groovy.json.JsonSlurper

plugins { id("com.android.application") }

val developmentApk = providers.gradleProperty("developmentApk").orNull == "1"

// -Pcompose=1: the app's plan has a date or time control, and its picker is Compose Material 3's (LLP 1115.001.000
// D1-D2; `agent-android.mjs` reads the plan's uses). Without it the framework's dialogs show, and Compose is not linked.
val compose = providers.gradleProperty("compose").orNull == "1"
if (compose) apply(plugin = "org.jetbrains.kotlin.plugin.compose")
// -Pvideo=1: the plan has a `video` or an `audio`, or the build is a development one, and Media3's player (`:video`) is
// linked (LLP 1042.001 D2). Without it the APK carries none of Media3.
val video = providers.gradleProperty("video").orNull == "1"
// -Pwidgets=1: app.json declares a widget Android draws, one with a `system*` family (LLP 1126.003 D1, D9), and
// `:widgets` is linked; the build below makes each kind's provider. Without it the APK has none of it.
val widgets = providers.gradleProperty("widgets").orNull == "1"

val app = providers.gradleProperty("app").get()
val appDir = providers.gradleProperty("appDir").orNull?.let { file(it) } ?: rootDir.resolve("../../apps/$app")
// A game's manifest is the one its shells were generated with (`-PappManifest`, its `.shells/app.json`), which has
// its `app` identity; the rest of the app (`modules/android`, `.exact/`) is in its directory.
val manifestFile = providers.gradleProperty("appManifest").orNull?.let { file(it) } ?: appDir.resolve("app.json")
val manifest = JsonSlurper().parse(manifestFile) as Map<*, *>
val androidHost = (manifest["host"] as Map<*, *>?)?.get("android") as Map<*, *>?
val packageId = providers.gradleProperty("applicationId").get()
val backgroundPlayback = video && manifest["audio_session"] == "playback"
// The manifest's `orientation` as Chromium maps an installed web app's: `-primary` is the device's own way up, a
// bare `portrait` or `landscape` either way up by the sensor. `natural` is resolved in MainActivity.
val screenOrientation = mapOf(
    "any" to "fullUser", "natural" to "nosensor", "portrait" to "sensorPortrait", "portrait-primary" to "portrait",
    "portrait-secondary" to "reversePortrait", "landscape" to "sensorLandscape", "landscape-primary" to "landscape",
    "landscape-secondary" to "reverseLandscape",
)[manifest["orientation"]] ?: "unspecified"
// The manifest's `host.android.minimumOS`, the oldest Android the app runs on, as its API level. Below 12 the APK keeps
// the reader's Android 10 and 11 code (proguard-legacy.pro); otherwise R8 leaves none of it.
val minimumOS = (androidHost?.get("minimumOS") as String?) ?: "12"
val minimumApi = mapOf("10" to 29, "11" to 30, "12" to 31, "13" to 33, "14" to 34, "15" to 35, "16" to 36)[minimumOS]
    ?: throw GradleException("app.json host.android.minimumOS \"$minimumOS\": name an Android release from 10 to 16")

// Each app builds into a directory of its own (`-PappBuildDir`, else build/<app>): two apps' libraries, assets and APKs
// never mix, and two builds at once write nothing in common (agent-android.mjs gives the reader one too, `canvasBuildDir`).
val out = providers.gradleProperty("appBuildDir").orNull?.let { file(it) } ?: projectDir.resolve("build/$app")
layout.buildDirectory = out

android {
    namespace = "dev.exact.app"
    // The NDK the app's library was linked with (`-PndkPath`, agent-android.mjs), whose strip the APK's library goes
    // through; without it AGP looks for its own default NDK and, missing it, packages the library unstripped.
    providers.gradleProperty("ndkPath").orNull?.let { path ->
        ndkPath = path
        ndkVersion = file("$path/source.properties").readLines().first { it.startsWith("Pkg.Revision") }.substringAfter("=").trim()
    }
    compileSdk { version = release(37) { minorApiLevel = 2 } }
    buildToolsVersion = "37.0.0"
    defaultConfig {
        applicationId = packageId
        minSdk = minimumApi
        targetSdk = 36
        // CI's version inputs are shared with Apple builds (LLP 1030.003 D6).
        versionCode = providers.environmentVariable("EXACT_BUILD_NUMBER").orElse("1").get().toInt().also {
            require(it in 1..2100000000) { "EXACT_BUILD_NUMBER must be an integer from 1 to 2100000000" }
        }
        versionName = providers.environmentVariable("EXACT_VERSION").orElse("0.1.0").get()
        ndk { abiFilters += providers.gradleProperty("abis").orElse("arm64-v8a").get().split(',') }
        manifestPlaceholders["audioSession"] = manifest["audio_session"] as String? ?: "ambient"
        manifestPlaceholders["appName"] = providers.gradleProperty("appName").get()
        manifestPlaceholders["screenOrientation"] = screenOrientation
        manifestPlaceholders["exactDevelopment"] = providers.gradleProperty("exactDevelopment").orNull == "1"
        // The launcher icon (`android:icon` and `android:roundIcon`) and the TV banner among the resources below, as
        // agent-android.mjs names them. With no icon the system draws its default one; with no banner the TV launcher
        // shows the shell's plain one (res/drawable/banner.xml).
        manifestPlaceholders["appIcon"] = providers.gradleProperty("appIcon").orNull ?: "@null"
        manifestPlaceholders["appBanner"] = providers.gradleProperty("appBanner").orNull ?: "@drawable/banner"
        // The launch window in the manifest's launch colours and with its `host.android.launchImage`
        // (agent-android.mjs `androidLaunch`); without either the reader's, the platform's (LLP 1115.001 D3.6).
        manifestPlaceholders["appTheme"] = providers.gradleProperty("appTheme").orNull ?: "@style/ExactLaunch"
    }
    val upload = providers.environmentVariable("EXACT_KEYSTORE").orNull?.let { path ->
        signingConfigs.create("upload") {
            storeFile = file(path)
            storePassword = providers.environmentVariable("EXACT_KEYSTORE_PASSWORD").get()
            keyAlias = providers.environmentVariable("EXACT_KEY_ALIAS").get()
            keyPassword = providers.environmentVariable("EXACT_KEY_PASSWORD").get()
        }
    }
    lint { checkReleaseBuilds = !developmentApk }
    buildTypes {
        release {
            isMinifyEnabled = true
            if (developmentApk) proguardFiles("proguard-development.pro")
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (minimumApi < 31) proguardFiles("proguard-legacy.pro")
            signingConfig = upload ?: signingConfigs.getByName("debug")
        }
    }
    // The app's Rust library (as libexactcanvas.so) and its assets (the reader copies `app/` out of the APK once).
    sourceSets["main"].jniLibs.directories.add(out.resolve("rust/jniLibs").path)
    sourceSets["main"].assets.directories.add(out.resolve("rust/assets").path)
    // The icon and banner agent-android.mjs makes from the manifest's `icons` (LLP 1163 D6).
    sourceSets["main"].res.directories.add(out.resolve("icons/res").path)
    sourceSets["main"].res.directories.add(out.resolve("launch/res").path)
    // The notification channels, their strings and sounds, and the small icon, for an app that grants notifications
    // (agent-android.mjs `androidNotificationResources`, LLP 1133.004.000 D1, D3).
    sourceSets["main"].res.directories.add(out.resolve("notify/res").path)
    sourceSets["main"].kotlin.directories.add(if (compose) "src/compose/java" else "src/nocompose/java")
    sourceSets["main"].kotlin.directories.add(when {
        backgroundPlayback -> "src/playback/java"
        video -> "src/video/java"
        else -> "src/novideo/java"
    })
    // The app's Kotlin hatches and module roster (LLP 1115.001.000 D4) in its `modules/android`; -PmodulesDir lets a
    // component fixture compile the app's actual provider without copying its Kotlin.
    val modulesDir = providers.gradleProperty("modulesDir").orNull?.let { file(it) } ?: appDir.resolve("modules/android")
    modulesDir.takeIf { it.isDirectory }?.let { sourceSets["main"].kotlin.directories.add(it.path) }
    // The modules' implementations the app binds (LLP 1133.001 D5): their Kotlin, and the roster naming them that
    // `agent-android.mjs` writes (`-PextensionDirs`).
    providers.gradleProperty("extensionDirs").orNull?.split(',')?.filter { it.isNotEmpty() }?.forEach { sourceSets["main"].kotlin.directories.add(it) }
    // The runtime permissions the app's grant lines derive (`-Ppermissions`, LLP 1133.002 D6), and the queries and
    // components its packages' manifest fragments declare (`-PmanifestFragment`, checked and written by
    // scripts/android-manifest.mjs, LLP 1133.030 D9, LLP 1133.026 D19 item 3; with the share target's filters on the
    // app's activity, scripts/share-target.mjs, LLP 1133.024 D5), merged into the shell's manifest from the release source set's: an app that grants no device asks
    // for none, and one that uses no such package runs nothing beyond the shell's activity.
    val fragment = providers.gradleProperty("manifestFragment").orNull?.let { file(it).readText() }?.takeIf { it.isNotBlank() }
    // The kernel's symbol vocabulary and Material shapes (LLP 1116.007 D3.1, LLP 1115.001 D14), including every role
    // for computed dynamic/pinned items: a shortcut's and a control's icon. The lookup also keeps static and runtime
    // resolution the same; the shortcuts below write a drawable for each role.
    val schema = JsonSlurper().parse(rootDir.resolve("../../kernel/tables/schema.json")) as Map<*, *>
    val symbols = (schema["symbols"] as List<*>).map { it as List<*> }
    val glyphs = (schema["symbolGlyphs"] as List<*>).map { it as List<*> }.filter { it[1] == "material" }
    val symbolKey = { name: String -> if (name.startsWith("sf/")) name else name.replace('.', '-').replace('_', '-') }
    val symbolNames = linkedMapOf<String, String>()
    symbols.forEach { symbolNames[it[0] as String] = it[0] as String }
    symbols.forEach { row ->
        val role = row[0] as String
        val apple = row[1] as String
        if (apple.isNotEmpty()) {
            symbolNames.putIfAbsent(symbolKey(apple), role)
            symbolNames.putIfAbsent("sf/$apple", role)
        }
    }
    (schema["symbolAliases"] as List<*>).forEach {
        val row = it as List<*>
        symbolNames[row[0] as String] = row[1] as String
    }
    val iconName = { role: String -> "exact_shortcut_symbol_${role.replace('-', '_')}" }
    // The app's controls (LLP 1126.004 D5): per row the bake offers here (`compat.inputs.controls`,
    // bake/src/controls.rs) a Quick Settings tile, a `ControlTile` subclass named by the row's kind in UTF-8 hex so a
    // placed tile keeps its row when the rows are reordered, with the row's title and its symbol's Material glyph (the
    // app's icon without one). The services join the manifest below.
    val controlServices = StringBuilder()
    run {
        val compat = out.resolve("rust/bake/compat.json").takeIf { it.isFile }?.let { JsonSlurper().parse(it) as Map<*, *> }
        val rows = ((compat?.get("inputs") as Map<*, *>?)?.get("controls") as List<*>?).orEmpty().map { it as Map<*, *> }
            .filter { ((it["placement"] as Map<*, *>)["offered"] as List<*>?).orEmpty().isNotEmpty() }
        val dir = layout.buildDirectory.dir("controls").get().asFile
        dir.deleteRecursively()
        if (rows.isEmpty()) return@run
        val kotlin = dir.resolve("java/dev/exact/app").apply { mkdirs() }
        val values = dir.resolve("res/values").apply { mkdirs() }
        val attr = { v: String -> v.replace("&", "&amp;").replace("<", "&lt;").replace("\"", "&quot;") }
        // A string resource's own escapes first, so a quotation mark stays one, then the XML's.
        val text = { v: String ->
            v.replace("\\", "\\\\").replace("\"", "\\\"").replace("'", "\\'").replace(Regex("^([@?])"), "\\\\$1")
                .replace("&", "&amp;").replace("<", "&lt;")
        }
        val literal = { v: String? ->
            v?.let { "\"" + it.replace("\\", "\\\\").replace("\"", "\\\"").replace("$", "\\$").replace("\n", "\\n").replace("\r", "\\r") + "\"" } ?: "null"
        }
        val strings = StringBuilder()
        val source = StringBuilder("package dev.exact.app\n")
        for (row in rows) {
            val control = row["control"] as Map<*, *>
            val kind = control["kind"] as String
            val name = "ExactControl_" + kind.toByteArray(Charsets.UTF_8).joinToString("") { "%02x".format(it) }
            val intent = (((row["placement"] as Map<*, *>)["offered"] as List<*>)[0] as Map<*, *>)["intent"] as String
            // A `.fill` name without its own glyph takes its outline's. A tile with no icon at all may be left out of the
            // Quick Settings editor, so one without a glyph takes the app's icon, or the platform's default one.
            val symbol = control["symbol"] as String
            val role = symbolNames[symbolKey(symbol)] ?: symbolNames[symbolKey(symbol.removeSuffix(".fill"))]
            val appIcon = providers.gradleProperty("appIcon").orNull ?: "@android:drawable/sym_def_app_icon"
            val icon = "\n            android:icon=\"${role?.let { "@drawable/${iconName(it)}" } ?: appIcon}\""
            strings.append("    <string name=\"${name.lowercase()}_title\">${text(control["title"] as String)}</string>\n")
            source.append("\n/** The `$kind` control's tile. */\nclass $name : dev.exact.canvas.ControlTile(${literal(kind)}, ${literal(row["digest"] as String)}, " +
                "${literal(intent)}, ${literal(control["url"] as String?)}, ${literal((control["authentication"] as String?) ?: "required")}, ${literal(control["description"] as String?)})\n")
            controlServices.append("        <service android:name=\"dev.exact.app.$name\" android:exported=\"true\"\n" +
                "            android:label=\"@string/${name.lowercase()}_title\"$icon\n" +
                "            android:permission=\"android.permission.BIND_QUICK_SETTINGS_TILE\">\n" +
                "            <intent-filter><action android:name=\"android.service.quicksettings.action.QS_TILE\" /></intent-filter>\n" +
                "            <meta-data android:name=\"android.service.quicksettings.TOGGLEABLE_TILE\" android:value=\"false\" />\n" +
                "        </service>\n")
        }
        values.resolve("exact_controls.xml").writeText("<resources>\n$strings</resources>\n")
        kotlin.resolve("Controls.kt").writeText(source.toString())
        sourceSets["main"].res.directories.add(dir.resolve("res").path)
        sourceSets["main"].kotlin.directories.add(dir.resolve("java").path)
    }
    // The app's widgets (LLP 1126.003 D9): per kind a provider subclass (Android names a provider by its class), its
    // `appwidget-provider` XML with D4's sizes, its initial and preview layout, and its receiver in the `:widgets`
    // process; and the rows `:widgets` reads (res/raw/exact_widgets.json). The receivers join the manifest below.
    val widgetReceivers = StringBuilder()
    if (widgets) {
        val dir = layout.buildDirectory.dir("widgets").get().asFile
        dir.deleteRecursively()
        val res = dir.resolve("res")
        listOf("xml", "layout", "values", "raw").forEach { res.resolve(it).mkdirs() }
        val kotlin = dir.resolve("java/dev/exact/app").apply { mkdirs() }
        val attr = { v: String -> v.replace("&", "&amp;").replace("<", "&lt;").replace("\"", "&quot;") }
        val text = { v: String -> attr(v).replace("\\", "\\\\").replace("'", "\\'").replace(Regex("^([@?])"), "\\\\$1") }
        // Apple's families as cell spans (D4); `systemExtraLarge` is iPad's, a 4×4 here as `systemLarge`.
        val cells = mapOf("systemSmall" to (2 to 2), "systemMedium" to (4 to 2), "systemLarge" to (4 to 4), "systemExtraLarge" to (4 to 4), "systemExtraLargePortrait" to (4 to 4))
        // The settings screen offers fixed choices only (LLP 1126.003 D7). An entity parameter is left out, so its
        // field stays none here, and a row with no parameter left has no settings.
        val rows = (manifest["widgets"] as List<*>?).orEmpty().map { it as Map<*, *> }
            .filter { row -> (row["families"] as List<*>?).orEmpty().any { (it as String).startsWith("system") } }
            .map { row ->
                val configuration = row["configuration"] as Map<*, *>? ?: return@map row
                val fixed = (configuration["parameters"] as List<*>).filter { (it as Map<*, *>)["type"] == "enum" }
                if (fixed.isEmpty()) row - "configuration" else row + ("configuration" to configuration + ("parameters" to fixed))
            }
        val strings = StringBuilder()
        rows.forEachIndexed { i, row ->
            val kind = row["kind"] as String
            val title = (row["title"] as String?) ?: kind
            val description = (row["description"] as String?) ?: title
            val (w, h) = (row["families"] as List<*>).mapNotNull { cells[it as String] }.minBy { it.first * it.second }
            val dp = { n: Int -> "${70 * n - 30}dp" }
            val configured = row["configuration"] != null
            strings.append("    <string name=\"exact_widget_${i}_title\">${text(title)}</string>\n")
            strings.append("    <string name=\"exact_widget_${i}_description\">${text(description)}</string>\n")
            res.resolve("xml/exact_widget_$i.xml").writeText("<appwidget-provider xmlns:android=\"http://schemas.android.com/apk/res/android\"\n" +
                "    android:minWidth=\"${dp(w)}\" android:minHeight=\"${dp(h)}\" android:targetCellWidth=\"$w\" android:targetCellHeight=\"$h\"\n" +
                "    android:minResizeWidth=\"${dp(w)}\" android:minResizeHeight=\"${dp(h)}\" android:resizeMode=\"horizontal|vertical\"\n" +
                "    android:updatePeriodMillis=\"1800000\" android:widgetCategory=\"home_screen\"\n" +
                "    android:description=\"@string/exact_widget_${i}_description\"\n" +
                "    android:initialLayout=\"@layout/exact_widget_${i}_initial\" android:previewLayout=\"@layout/exact_widget_${i}_initial\"" +
                (if (configured) "\n    android:configure=\"dev.exact.widgets.Configure\" android:widgetFeatures=\"reconfigurable|configuration_optional\"" else "") +
                " />\n")
            // Until the first render: the widget's background in the system's colours, with the row's title (D9).
            res.resolve("layout/exact_widget_${i}_initial.xml").writeText("<FrameLayout xmlns:android=\"http://schemas.android.com/apk/res/android\"\n" +
                "    android:id=\"@android:id/background\" android:layout_width=\"match_parent\" android:layout_height=\"match_parent\"\n" +
                "    android:background=\"@drawable/exact_widget_background\" android:clipToOutline=\"true\" android:padding=\"16dp\">\n" +
                "    <TextView android:layout_width=\"wrap_content\" android:layout_height=\"wrap_content\" android:text=\"@string/exact_widget_${i}_title\"\n" +
                "        android:textAppearance=\"@android:style/TextAppearance.DeviceDefault.Medium\" android:textColor=\"@color/exact_widget_role_4\" />\n" +
                "</FrameLayout>\n")
            kotlin.resolve("ExactWidget$i.kt").writeText("package dev.exact.app\n\n/** The `$kind` widget's provider (LLP 1126.003 D9). */\nclass ExactWidget$i : dev.exact.widgets.Provider()\n")
            widgetReceivers.append("        <receiver android:name=\"dev.exact.app.ExactWidget$i\" android:exported=\"false\" android:process=\":widgets\"\n" +
                "            android:enabled=\"@bool/exact_widgets_enabled\" android:label=\"@string/exact_widget_${i}_title\">\n" +
                "            <intent-filter>\n" +
                listOf("android.appwidget.action.APPWIDGET_UPDATE", "android.intent.action.LOCALE_CHANGED", "android.intent.action.TIME_SET", "android.intent.action.TIMEZONE_CHANGED")
                    .joinToString("") { "                <action android:name=\"$it\" />\n" } +
                "            </intent-filter>\n" +
                "            <meta-data android:name=\"android.appwidget.provider\" android:resource=\"@xml/exact_widget_$i\" />\n" +
                "            <meta-data android:name=\"dev.exact.widget.kind\" android:value=\"${attr(kind)}\" />\n" +
                "        </receiver>\n")
        }
        res.resolve("values/exact_widgets.xml").writeText("<resources>\n$strings</resources>\n")
        res.resolve("raw/exact_widgets.json").writeText(groovy.json.JsonOutput.toJson(rows))
        sourceSets["main"].res.directories.add(res.path)
        sourceSets["main"].kotlin.directories.add(dir.resolve("java").path)
    }
    // Hardware and software features from grant lines (`-PandroidFeatures`, LLP 1133.022 D18, LLP 1133.037 D17).
    val lineFeatures = providers.gradleProperty("androidFeatures").orNull?.split(',')?.filter { it.isNotBlank() }.orEmpty()
    val linePermissions = providers.gradleProperty("permissions").orNull?.split(',')?.filter { it.isNotEmpty() }.orEmpty()
    val playbackPermissions = if (backgroundPlayback) listOf(
        "android.permission.FOREGROUND_SERVICE",
        "android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK",
        "android.permission.WAKE_LOCK",
    ) else emptyList()
    (linePermissions + playbackPermissions).distinct()
        .takeIf { it.isNotEmpty() || fragment != null || lineFeatures.isNotEmpty() || widgetReceivers.isNotEmpty() || controlServices.isNotEmpty() }?.let { names ->
        val permissions = layout.buildDirectory.file("permissions/AndroidManifest.xml").get().asFile
        permissions.parentFile.mkdirs()
        // CAMERA implies the back camera and autofocus as required features, which would filter the app from a
        // device without them; each is declared not required (LLP 1133.008 D6).
        val features = (if ("android.permission.CAMERA" in names) listOf("camera.any", "camera", "camera.autofocus", "camera.flash").map { "android.hardware.$it required=false" } else emptyList()) + lineFeatures
        // A retired permission is needed only when the app runs on an API that still uses it.
        val permission = { line: String ->
            val parts = line.split(' ')
            val attributes = parts.drop(1).joinToString("") {
                val (key, value) = it.split('=', limit = 2)
                require(key == "maxSdkVersion") { "Unknown permission attribute: $key" }
                " android:$key=\"$value\""
            }
            val lastApi = parts.drop(1).firstOrNull { it.startsWith("maxSdkVersion=") }?.substringAfter('=')?.toInt()
            if (lastApi != null && minimumApi > lastApi) "" else "    <uses-permission android:name=\"${parts[0]}\"$attributes />\n"
        }
        // The reader's foreground services the lines declare (`-PforegroundServices`, `{kind: {purpose, types}}`, LLP
        // 1133.033 D2), each with every type its lines give it: a type the manifest lacks throws at `startForeground`.
        // The purpose is the notification's title and its channel's name (D4).
        val escape = { t: String -> t.replace("&", "&amp;").replace("<", "&lt;").replace("\"", "&quot;") }
        @Suppress("UNCHECKED_CAST")
        val services = (providers.gradleProperty("foregroundServices").orNull?.let { JsonSlurper().parseText(it) } as Map<String, Map<String, Any?>>?).orEmpty()
        val service = { kind: String, declared: Map<String, Any?> ->
            "        <service android:name=\"dev.exact.canvas.ForegroundService\$${kind.replaceFirstChar { it.uppercase() }}\" android:exported=\"false\"\n" +
                "            android:foregroundServiceType=\"${(declared["types"] as List<*>).joinToString("|")}\">\n" +
                "            <meta-data android:name=\"dev.exact.purpose\" android:value=\"${escape(declared["purpose"]?.toString() ?: "")}\" />\n        </service>\n"
        }
        val playbackService = if (backgroundPlayback) """
            |        <service android:name="dev.exact.video.PlaybackService" android:exported="false"
            |            android:foregroundServiceType="mediaPlayback">
            |            <intent-filter>
            |                <action android:name="androidx.media3.session.MediaSessionService" />
            |            </intent-filter>
            |        </service>
            |
        """.trimMargin() else ""
        // The services join the fragment's `<application>` when it has one: a manifest holds one.
        val manifestChildren = { fragment: String?, services: String ->
            when {
                services.isEmpty() -> fragment?.let { "$it\n" } ?: ""
                fragment?.contains("    </application>") == true -> fragment.replace("    </application>", "$services    </application>") + "\n"
                else -> (fragment?.let { "$it\n" } ?: "") + "    <application>\n$services    </application>\n"
            }
        }
        val manifestOpen = "<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\" xmlns:tools=\"http://schemas.android.com/tools\">\n"
        permissions.writeText(manifestOpen +
            names.joinToString("") { permission(it) } +
            features.joinToString("") {
                val parts = it.split(' ')
                require(parts.size == 1 || (parts.size == 2 && parts[1] == "required=false")) { "Invalid feature: $it" }
                "    <uses-feature android:name=\"${parts[0]}\" android:required=\"${parts.size == 1}\" />\n"
            } +
            manifestChildren(fragment, services.entries.joinToString("") { service(it.key, it.value) } + playbackService + widgetReceivers + controlServices) + "</manifest>\n")
        sourceSets["release"].manifest.srcFile(permissions)
        if (backgroundPlayback) {
            val debug = permissions.parentFile.resolve("debug/AndroidManifest.xml")
            debug.parentFile.mkdirs()
            debug.writeText(manifestOpen + playbackPermissions.joinToString("") { permission(it) } +
                manifestChildren(null, playbackService) + "</manifest>\n")
            sourceSets["debug"].manifest.srcFile(debug)
        }
    }
    // The manifest's `shortcuts` as the launcher activity's static shortcuts (LLP 1133.023 D3, D17), in order: each
    // starts the activity with its location as the intent's data, which the reader takes as a link's. Labels and the
    // disabled message are string resources, as `shortcuts.xml` requires; an app with none has an empty list.
    run {
        val res = layout.buildDirectory.dir("shortcuts/res").get().asFile
        val items = (manifest["shortcuts"] as List<*>?).orEmpty().map { it as Map<*, *> }
        val attr = { v: String -> v.replace("&", "&amp;").replace("<", "&lt;").replace("\"", "&quot;") }
        // A string resource's own escapes: a backslash, an apostrophe, and `@` or `?` leading.
        val text = { v: String -> attr(v).replace("\\", "\\\\").replace("'", "\\'").replace(Regex("^([@?])"), "\\\\$1") }
        res.deleteRecursively()
        res.resolve("drawable").mkdirs()
        symbols.forEach { row ->
            val role = row[0] as String
            val glyph = glyphs.firstOrNull { it[0] == role }
            val path = (glyph?.get(2) ?: row[2]) as String
            val filled = (glyph?.get(3) as Boolean?) ?: role.endsWith("-fill")
            // A role's path is in a 24-unit box; Material's glyphs are in Material Symbols' `0 -960 960 960`.
            val material = glyph?.get(2) != null
            val (size, open, close) = if (!material) Triple(24, "", "") else Triple(960, "    <group android:translateY=\"960\">\n", "    </group>\n")
            // A role's own silhouette fills even-odd; Material's glyphs, nonzero.
            val paint = if (filled) "android:fillColor=\"#FF000000\"" + (if (!material) " android:fillType=\"evenOdd\"" else "") else
                "android:fillColor=\"#00000000\" android:strokeColor=\"#FF000000\" android:strokeWidth=\"${size / 12}\" " +
                    "android:strokeLineCap=\"round\" android:strokeLineJoin=\"round\""
            res.resolve("drawable/${iconName(role)}.xml").writeText(
                "<vector xmlns:android=\"http://schemas.android.com/apk/res/android\" android:width=\"24dp\" android:height=\"24dp\" " +
                    "android:viewportWidth=\"$size\" android:viewportHeight=\"$size\">\n" + open +
                    path.split(';').joinToString("") { "    <path android:pathData=\"${attr(it)}\" $paint />\n" } + close + "</vector>\n")
        }
        val strings = StringBuilder()
        val shortcuts = StringBuilder()
        items.forEachIndexed { i, item ->
            val name = item["name"] as String
            val symbol = (item["apple"] as Map<*, *>?)?.get("symbol") as String?
            val icon = symbol?.let { symbolNames[symbolKey(it)] }?.let { "@drawable/${iconName(it)}" }
            val disabled = (item["android"] as Map<*, *>?)?.get("disabledMessage") as String?
            strings.append("    <string name=\"exact_shortcut_${i}_short\">${text((item["short_name"] as String?) ?: name)}</string>\n")
            strings.append("    <string name=\"exact_shortcut_${i}_long\">${text(name)}</string>\n")
            disabled?.let { strings.append("    <string name=\"exact_shortcut_${i}_disabled\">${text(it)}</string>\n") }
            shortcuts.append("    <shortcut android:shortcutId=\"manifest-$i\" android:enabled=\"true\"\n" +
                "        android:shortcutShortLabel=\"@string/exact_shortcut_${i}_short\" android:shortcutLongLabel=\"@string/exact_shortcut_${i}_long\"" +
                (icon?.let { "\n        android:icon=\"$it\"" } ?: "") +
                (disabled?.let { "\n        android:shortcutDisabledMessage=\"@string/exact_shortcut_${i}_disabled\"" } ?: "") + ">\n" +
                "        <intent android:action=\"android.intent.action.VIEW\" android:targetPackage=\"$packageId\"\n" +
                "            android:targetClass=\"dev.exact.app.MainActivity\" android:data=\"${attr(item["url"] as String)}\" />\n" +
                "    </shortcut>\n")
        }
        res.resolve("xml").mkdirs()
        res.resolve("values").mkdirs()
        res.resolve("xml/exact_shortcuts.xml").writeText("<shortcuts xmlns:android=\"http://schemas.android.com/apk/res/android\">\n$shortcuts</shortcuts>\n")
        res.resolve("values/exact_shortcuts.xml").writeText("<resources>\n$strings</resources>\n")
        res.resolve("xml/exact_shortcut_symbols.xml").writeText("<symbols>\n" + symbolNames.entries.joinToString("") {
            "    <symbol name=\"${attr(it.key)}\" drawable=\"@drawable/${iconName(it.value)}\" />\n"
        } + "</symbols>\n")
        sourceSets["main"].res.directories.add(res.path)
    }
    // The token names `bun exact.mjs forma gen kotlin` wrote (LLP 1121 D12; never committed).
    appDir.resolve(".exact/forma/FormaTokens.kt").takeIf { it.isFile }?.let { sourceSets["main"].kotlin.directories.add(it.parent) }
    if (app == "shelf") sourceSets["test"].kotlin.directories.addAll(listOf(
        "../../../packages/quick-actions/android", "../../../packages/quick-actions/test/android",
    ))
    testOptions { unitTests { isIncludeAndroidResources = true } }
    if (compose) {
        buildFeatures { this.compose = true }
        sourceSets["test"].kotlin.directories.add("src/composeTest/java")
    }
    // An app's GPU modules (-PgpuModules=<file>,<file>…: the primary and each named one) are opened by path from the
    // extracted library directory and checked against the digests their bake recorded, so they are extracted and left
    // unstripped.
    val gpuModules = providers.gradleProperty("gpuModules").orNull?.split(',')?.filter { it.isNotEmpty() }.orEmpty()
    packaging {
        jniLibs {
            useLegacyPackaging = gpuModules.isNotEmpty()
            gpuModules.forEach { keepDebugSymbols += "**/$it" }
        }
    }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
}
// The Maven libraries the app and bound implementations name (`-Pmaven`, LLP 1133.008 D8 item 5), resolved with the
// reader's own AndroidX in this one graph. The graph is locked to the app's lock file (`-PmavenLock`, beside its other
// locks) and verified against its checksums (`agent-android.mjs` puts the app's verification file in place and passes
// `--dependency-verification=strict`); `--write-locks` there writes both. An app with none resolves as before.
val maven = providers.gradleProperty("maven").orNull?.split(',')?.filter { it.isNotEmpty() }.orEmpty()
if (maven.isNotEmpty()) {
    dependencyLocking {
        lockAllConfigurations()
        lockMode.set(LockMode.STRICT)
        lockFile.set(file(providers.gradleProperty("mavenLock").get()))
    }
    if (gradle.startParameter.isWriteDependencyLocks) {
        // Lock constraints can expose transitive edges absent from an unconstrained graph. Resolve with the
        // selected versions constrained before recording the lock, as the next strict build will resolve it.
        configurations.configureEach {
            incoming.beforeResolve {
                val unlocked = copyRecursive()
                unlocked.resolutionStrategy.deactivateDependencyLocking()
                shouldResolveConsistentlyWith(unlocked)
            }
        }
    }
}
dependencies {
    maven.forEach { implementation(it) }
    implementation(project(":canvas"))
    if (video) implementation(project(":video"))
    if (backgroundPlayback) {
        implementation(libs.media3.session)
        implementation(libs.media3.ui)
    }
    if (widgets) implementation(project(":widgets"))
    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
    if (compose) {
        implementation(platform(libs.compose.bom))
        implementation(libs.compose.material3)
        implementation("androidx.core:core:1.16.0")
    }
}
