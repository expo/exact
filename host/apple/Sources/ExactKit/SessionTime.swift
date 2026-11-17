// The date a session tells its runner (LLP 1027.000.000): at boot, before
// the first settle, and whenever the date, the zone's offset or the drive's
// clock moves.
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
import ExactCore

extension ExactSession {
    /// Sends the date, zone, locale and preferences after a boot.
    func tellTime() {
        if launchPlace.epoch != nil {
            tellAgentOffset()
        } else {
            toldOffset = nil
            toldEpoch = nil
            offsetHeld = nil
            followOffset()
        }
        apply(runtime.setPlace(locale: launchPlace.locale, timeZone: launchPlace.timeZone, seed: launchPlace.seed, formatLocale: launchPlace.formatLocale))
        tellPreferences()
        tellPage()
    }
    /// The machine's date at the clock's zero and its zone's offset, told
    /// when either moved: the offset, or the date by a second or more (the
    /// runner's own threshold). The date moves when the clock is set, and
    /// through a sleep, since the session's clock (`CACurrentMediaTime`,
    /// `mach_absolute_time`) stops while the system sleeps and the date does
    /// not. Runs before each advance, frame, native task timer, page notice
    /// and person's input (`applyInput`), so what each reads of the date is
    /// the date now; `dateChanged` runs it on a system notice.
    func followOffset() {
        guard let date = dateMoved() else { return }
        tellDate(date, soon: false)
    }
    /// Tells the runner `date`, or queues it behind the owner's current job
    /// (`soon`). One the runner refuses (busy, LLP 1072 T5, or held over a
    /// restart's reset) was not told, so it is forgotten: the next check
    /// tells the date again rather than wait for it to move by another second.
    private func tellDate(_ date: (epoch: Double, offset: Double), soon: Bool) {
        let tell: (Runtime) -> Batch = { $0.setTime(epochAtZero: date.epoch, utcOffset: date.offset) }
        if soon {
            if runtime.held { toldEpoch = nil } // (`tellSoon` holds it, and ResetGate's replay tells the page, which checks)
            tellSoon([tell])
            return
        }
        let batch = tell(runtime)
        guard !batch.busy, !batch.held else { toldEpoch = nil; return }
        apply(batch)
    }
    /// `followOffset`'s date, nil when neither moved. `offsetHeld` is the
    /// zone the offset was read in and when to read it again: until then, in
    /// that zone, a frame's check reads no calendar.
    private func dateMoved() -> (epoch: Double, offset: Double)? {
        guard launchPlace.epoch == nil else { return nil }
        let zone = TimeZone.autoupdatingCurrent, wall = Self.machineDate()
        let epoch = wall.timeIntervalSince1970 * 1000 - now()
        let moved = toldEpoch.map { abs(epoch - $0) >= 1000 } ?? true
        if !moved, let held = offsetHeld, held.zone == zone.identifier, wall < held.until { return nil }
        // An hour at most: a zone's rules can move its offset at a time that
        // is no daylight-saving transition.
        let hour = wall.addingTimeInterval(3600)
        offsetHeld = (zone.identifier, min(zone.nextDaylightSavingTimeTransition(after: wall) ?? hour, hour))
        let offset = Double(zone.secondsFromGMT(for: wall)) / 60
        guard moved || offset != toldOffset else { return nil }
        (toldEpoch, toldOffset) = (epoch, offset)
        return (epoch, offset)
    }
    /// A system notice that the date or the zone may have moved, for an app
    /// with no timer or frame to check it: told as a notice, as
    /// `formatLocaleChanged` tells the Region.
    func dateChanged() {
        guard booted, state != .destroyed, let date = dateMoved() else { return }
        tellDate(date, soon: dataPreparing && !Owner.shared.isServingMain)
    }
    /// The machine's date. A test sets it ahead to stand for a sleep, through
    /// which the session's clock (`CACurrentMediaTime`) stops and the date runs.
    nonisolated(unsafe) static var machineDate: () -> Date = Date.init
    /// The date at the clock's zero and the zone's offset now: the drive's
    /// under the agent (at the virtual instant), the machine's otherwise.
    func dateNow() -> (epoch: Double, offset: Double) {
        guard let epoch = launchPlace.epoch else {
            return (Self.machineDate().timeIntervalSince1970 * 1000 - now(), Double(TimeZone.autoupdatingCurrent.secondsFromGMT()) / 60)
        }
        let zone = TimeZone(identifier: launchPlace.timeZone) ?? TimeZone(secondsFromGMT: 0)!
        return (epoch, Double(zone.secondsFromGMT(for: Date(timeIntervalSince1970: (epoch + now()) / 1000))) / 60)
    }
    /// Under the agent, sends the drive's date at clock zero and its zone's
    /// offset at the current virtual time. Runs at boot and after every `clock`,
    /// so crossing a DST change updates it (LLP 1069.007 D2).
    func tellAgentOffset() {
        guard launchPlace.epoch != nil else { return }
        let date = dateNow()
        apply(runtime.setTime(epochAtZero: date.epoch, utcOffset: date.offset))
    }
    /// The date and where the viewer is (locale, zone, seed) before each
    /// boot or candidate: the runner reads them from its first settle, so
    /// an argument reading the zone's offset or the locale (a kept answer's
    /// identity) is the real one, not a default (or a reload's stale date)
    /// until `tellTime`. A running runner is not told.
    ///
    /// On the Mac, the process's layout direction too: AppKit fixes it at
    /// launch, including the standalone app's `host.macos.layoutDirection`.
    /// It draws the window's chrome in that direction, which also governs
    /// layout the author left unsaid, instead of the strings table's.
    func primeTime() {
        guard state != .destroyed else { return }
        let date = dateNow()
        runtime.setLaunchTime(epochAtZero: date.epoch, utcOffset: date.offset)
        runtime.setLaunchPlace(locale: launchPlace.locale, timeZone: launchPlace.timeZone, seed: launchPlace.seed, formatLocale: launchPlace.formatLocale)
        #if os(macOS)
        runtime.launchDirection(rtl: NSApp.map { $0.userInterfaceLayoutDirection == .rightToLeft })
        runtime.naturalText(true) // AppKit's text resolves an unsaid base per paragraph (LLP 1060.000 D3)
        #endif
        runtime.liveClock(clock == nil)
    }
    /// The notices that the place or the date changed, each with the center
    /// that posts it, which is the one to remove it from.
    ///
    /// The Region or the hour cycle the user forces: Foundation posts
    /// `NSLocale.currentLocaleDidChangeNotification` for either, and
    /// `formatLocale` is read again and told. The agent's is the drive's.
    ///
    /// The date and the zone (LLP 1027.000.000 D2): Foundation's
    /// `NSSystemClockDidChange`, "posted whenever the system clock is
    /// changed", and `NSSystemTimeZoneDidChange`. A sleep, after which the
    /// session's clock is behind the date: on macOS AppKit's
    /// `NSWorkspace.didWakeNotification`, which "the workspace posts when the
    /// device wakes from sleep" on its own `notificationCenter` only; on iOS
    /// an app is in the background while the device sleeps and becomes active
    /// again after (`UIApplication.didBecomeActiveNotification`), and UIKit's
    /// `significantTimeChangeNotification` adds midnight, a carrier time
    /// update and a daylight-saving change. (Apple Developer Documentation.)
    func observePlace() -> [(NotificationCenter, NSObjectProtocol)] {
        let center = NotificationCenter.default
        var observers = [(center, center.addObserver(forName: NSLocale.currentLocaleDidChangeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.formatLocaleChanged()
        })]
        var dated: [(NotificationCenter, Notification.Name)] = [(center, .NSSystemClockDidChange), (center, .NSSystemTimeZoneDidChange)]
        #if canImport(UIKit)
        dated += [(center, UIApplication.didBecomeActiveNotification), (center, UIApplication.significantTimeChangeNotification)]
        #else
        dated.append((NSWorkspace.shared.notificationCenter, NSWorkspace.didWakeNotification))
        #endif
        for (posting, name) in dated {
            observers.append((posting, posting.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in self?.dateChanged() }))
        }
        return observers
    }
    func formatLocaleChanged() {
        guard launchPlace.epoch == nil, state != .destroyed else { return }
        let read = SessionFacts.currentFormatLocale
        guard read != launchPlace.formatLocale else { return }
        launchPlace.formatLocale = read
        guard booted else { return } // `primeTime` tells the first boot
        let place = launchPlace
        let tell: (Runtime) -> Batch = { $0.setPlace(locale: place.locale, timeZone: place.timeZone, seed: place.seed, formatLocale: place.formatLocale) }
        if dataPreparing, !Owner.shared.isServingMain { tellSoon([tell]) } else { apply(tell(runtime)) }
    }
}
