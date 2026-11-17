package dev.exact.app

import android.app.Activity
import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import android.database.ContentObserver
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.format.DateFormat
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDefaults
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DisplayMode
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimeInput
import androidx.compose.material3.TimePicker
import androidx.compose.material3.TimePickerDialog
import androidx.compose.material3.TimePickerDialogDefaults
import androidx.compose.material3.TimePickerDisplayMode
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.core.graphics.ColorUtils
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import dev.exact.canvas.ExactPicker
import dev.exact.canvas.Pickers
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneOffset

/** The plan has a date or time control: its picker is Compose Material 3's (LLP 1115.001.000 D1). */
internal val materialPickers: Pickers? = MaterialPickers

/**
 * Material 3's `DatePickerDialog` and `TimePickerDialog`, as a Compose app shows them. Each opens in a
 * `ComposeView` added to the activity for as long as it is up; the dialog is a window of its own above the page.
 * `datetime-local` asks for the day, then the time.
 */
private object MaterialPickers : Pickers {
    /** The reader's activity is a plain Activity: Compose looks for a lifecycle and a saved state on the view tree.
     * One for each picker, on its own view: resumed while it is up, destroyed as it closes, which disposes it. */
    private class Owner : SavedStateRegistryOwner {
        private val registry = LifecycleRegistry(this)
        private val saved = SavedStateRegistryController.create(this)
        override val lifecycle: Lifecycle get() = registry
        override val savedStateRegistry get() = saved.savedStateRegistry
        init {
            saved.performRestore(null)
            registry.currentState = Lifecycle.State.RESUMED
        }
        fun destroy() { registry.currentState = Lifecycle.State.DESTROYED }
    }

    override fun show(activity: Activity, picker: ExactPicker, drawn: () -> Unit, answer: (String?) -> Unit): () -> Unit {
        val owner = Owner()
        val view = ComposeView(activity)
        view.setViewTreeLifecycleOwner(owner)
        view.setViewTreeSavedStateRegistryOwner(owner)
        var told = false
        val first = { if (!told) { told = true; drawn() } }
        var closed = false
        val close = { chosen: String? ->
            if (!closed) {
                closed = true
                (view.parent as? ViewGroup)?.removeView(view)
                owner.destroy()
                answer(chosen)
            }
        }
        view.setContent { Theme(activity, picker.accent) { Picker(picker, first, close) } }
        activity.addContentView(view, FrameLayout.LayoutParams(1, 1))
        return { close(null) }
    }
}

@Composable
private fun Theme(activity: Activity, accent: Int?, content: @Composable () -> Unit) {
    // The activity keeps running across night mode (`configChanges`); the composition local is what recomposes then.
    val dark = LocalConfiguration.current.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
    MaterialTheme(colorScheme = pickerColorScheme(activity, dark, accent), content = content)
}

/** Dynamic colours where available, else Material's baseline, as the page's Palette reads (LLP 1115.001.003 D2).
 * An authored accent supplies the primary roles' hue and chroma at Material's tones, including before API 31. */
internal fun pickerColorScheme(context: Context, dark: Boolean, accent: Int?): ColorScheme {
    val dynamic = if (Build.VERSION.SDK_INT < 31) null else try {
        if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
    } catch (_: Resources.NotFoundException) {
        null
    }
    val device = dynamic ?: if (dark) darkColorScheme() else lightColorScheme()
    return if (accent == null) device else {
        val hct = FloatArray(3)
        ColorUtils.colorToM3HCT(accent, hct)
        val tone = { t: Float -> Color(ColorUtils.M3HCTToColor(hct[0], hct[1], t)) }
        if (dark) device.copy(primary = tone(80f), onPrimary = tone(20f), primaryContainer = tone(30f), onPrimaryContainer = tone(90f), inversePrimary = tone(40f), surfaceTint = tone(80f))
        else device.copy(primary = tone(40f), onPrimary = tone(100f), primaryContainer = tone(90f), onPrimaryContainer = tone(10f), inversePrimary = tone(80f), surfaceTint = tone(40f))
    }
}

@Composable
private fun Picker(picker: ExactPicker, drawn: () -> Unit, done: (String?) -> Unit) {
    var day by remember { mutableStateOf<LocalDate?>(null) }
    if (picker.kind == "time" || day != null) {
        TimeDialog(picker, day, drawn, { done("") }) { t -> done(t?.let { picker.value(day, it) }) }
    } else {
        DateDialog(picker, drawn, { done("") }) { d ->
            when {
                d == null -> done(null)
                picker.kind == "date" -> done(picker.value(d, null))
                else -> day = d
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DateDialog(picker: ExactPicker, drawn: () -> Unit, clear: () -> Unit, done: (LocalDate?) -> Unit) {
    // Material's state refuses a month or a selection outside its years: they run from the first allowed year to
    // the last, else Material's own, widened to the day shown and the day held.
    val shown = picker.day()
    val held = picker.heldDay
    val from = picker.firstDay?.year ?: minOf(DatePickerDefaults.YearRange.first, shown.year, held?.year ?: shown.year)
    val to = maxOf(from, picker.lastDay?.year ?: maxOf(DatePickerDefaults.YearRange.last, shown.year, held?.year ?: shown.year))
    val state = rememberDatePickerState(
        initialSelectedDateMillis = held?.takeIf { it.year in from..to }?.let(::utc),
        initialDisplayedMonthMillis = utc(shown.withDayOfMonth(1)),
        yearRange = from..to,
        initialDisplayMode = if (picker.input) DisplayMode.Input else DisplayMode.Picker,
        selectableDates = object : SelectableDates {
            override fun isSelectableDate(utcTimeMillis: Long) = picker.allows(Instant.ofEpochMilli(utcTimeMillis).atZone(ZoneOffset.UTC).toLocalDate())
        },
    )
    val chosen = state.selectedDateMillis?.let { Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate() }
    // The held day may be one the control no longer allows: OK is for an allowed one.
    val ok = chosen != null && if (picker.kind == "date") picker.allows(picker.value(chosen, null)) else picker.allows(chosen)
    DatePickerDialog(
        onDismissRequest = { done(null) },
        confirmButton = { TextButton(onClick = { done(chosen) }, enabled = ok) { Text(stringResource(android.R.string.ok)) } },
        dismissButton = { Dismiss(picker, clear) { done(null) } },
    ) {
        val title = picker.title
        DatePicker(
            state = state,
            modifier = Modifier.drawWithContent { drawContent(); drawn() },
            title = if (title == null) {
                { DatePickerDefaults.DatePickerTitle(state.displayMode, Modifier.padding(TitlePadding)) }
            } else {
                { Text(title, Modifier.padding(TitlePadding)) }
            },
            showModeToggle = picker.modeToggle,
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TimeDialog(picker: ExactPicker, day: LocalDate?, drawn: () -> Unit, clear: () -> Unit, done: (LocalTime?) -> Unit) {
    val t = picker.time()
    val system = deviceHour24()
    val hour24 = picker.hour24 ?: system
    val state = rememberTimePickerState(t.hour, t.minute, hour24)
    // The state takes its clock only as it is made: the setting changing under an open dial is told to it.
    LaunchedEffect(hour24) { state.is24hour = hour24 }
    var mode by remember { mutableStateOf(if (picker.input) TimePickerDisplayMode.Input else TimePickerDisplayMode.Picker) }
    val title = picker.title
    // Material's clock offers no seconds: the held value's stay where its hour and minute are kept.
    val chosen = picker.time(state.hour, state.minute)
    TimePickerDialog(
        onDismissRequest = { done(null) },
        confirmButton = {
            TextButton(onClick = { done(chosen) }, enabled = picker.allows(picker.value(day, chosen))) { Text(stringResource(android.R.string.ok)) }
        },
        title = { if (title == null) TimePickerDialogDefaults.Title(mode) else Text(title) },
        modeToggleButton = if (picker.modeToggle) {
            {
                TimePickerDialogDefaults.DisplayModeToggle(
                    onDisplayModeChange = { mode = if (mode == TimePickerDisplayMode.Picker) TimePickerDisplayMode.Input else TimePickerDisplayMode.Picker },
                    displayMode = mode,
                )
            }
        } else null,
        dismissButton = { Dismiss(picker, clear) { done(null) } },
    ) {
        val drawing = Modifier.drawWithContent { drawContent(); drawn() }
        if (mode == TimePickerDisplayMode.Picker) TimePicker(state, drawing) else TimeInput(state, drawing)
    }
}

/** Settings' "Use 24-hour format", else the locale's clock, followed while the picker is up: the setting is no
 * part of a configuration, so it is watched as the activity watches it. */
@Composable
private fun deviceHour24(): Boolean {
    val context = LocalContext.current
    val configuration = LocalConfiguration.current
    var hour24 by remember(configuration) { mutableStateOf(DateFormat.is24HourFormat(context)) }
    DisposableEffect(context) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) { hour24 = DateFormat.is24HourFormat(context) }
        }
        val uri = Settings.System.getUriFor(Settings.System.TIME_12_24)
        context.contentResolver.registerContentObserver(uri, false, observer)
        onDispose { context.contentResolver.unregisterContentObserver(observer) }
    }
    return hour24
}

/** Cancel, and Clear before it where the picker offers it (`ExactPicker.clear`). */
@Composable
private fun Dismiss(picker: ExactPicker, clear: () -> Unit, cancel: () -> Unit) {
    Row {
        if (picker.clear) TextButton(onClick = clear) { Text(stringResource(dev.exact.canvas.R.string.exact_clear)) }
        TextButton(onClick = cancel) { Text(stringResource(android.R.string.cancel)) }
    }
}

/** Where Material's own date picker places its title (`DatePickerDefaults`' padding, 1.4.0). */
private val TitlePadding = PaddingValues(start = 24.dp, end = 12.dp, top = 16.dp)

private fun utc(day: LocalDate): Long = day.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli()
