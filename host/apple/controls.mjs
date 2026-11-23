// @ref LLP 1126.004 D2, D5 — the Swift of an app's controls on iOS and iPadOS: each offered row of the bake's
// `compat.inputs.controls` (bake/src/controls.rs) as an intent, compiled into both the app's intents module
// (intents.mjs) and the widget extension, and, in the extension alone, a `ControlWidget` whose button runs it.
//
// A URL control's intent is an `OpenIntent`: the system opens the app and runs it there, and it hands the
// location to the app's open-location channel (`exact.contentIndex.open`, ShortcutItems.swift), which queues it
// as the launch location before boot and navigates after. A tool control's intent is a `LiveActivityIntent`,
// which Apple runs in the app's process; there it calls `ExactTools.press`, the tool door's press, directly
// with the compiled kind and digest. The app compiles it against ExactToolDoor's module; the extension cannot
// import that module, so its copy refuses without dispatch. Neither intent is discoverable, and nothing else
// names their types.
// [Apple: creating controls](https://developer.apple.com/documentation/widgetkit/creating-controls-to-perform-actions-across-the-system)

/** The controls this build places on `platform`: the bake's rows whose placement offers a tier. */
export const offeredControls = (compat, platform) => platform === 'ios'
  ? (compat?.inputs?.controls ?? []).filter(c => c.placement?.offered?.length)
  : [];

// JSON's escapes are Swift's but for `\uXXXX`, which Swift writes `\u{XXXX}`; an escaped backslash before a `u` is text.
const literal = text => JSON.stringify(String(text)).replace(/((?:^|[^\\])(?:\\\\)*)\\u([0-9a-f]{4})/g, (_, before, h) => `${before}\\u{${h}}`);
// The row's kind is its identity: UTF-8 hex keeps generated names distinct and stable under reordering.
const hex = kind => Buffer.from(kind, 'utf8').toString('hex');
const intentName = c => `${c.control.url !== undefined ? 'ControlOpen' : 'ControlPress'}_${hex(c.control.kind)}`;
const authentication = word => ({ local: '.requiresLocalDeviceAuthentication', always: '.alwaysAllowed' })[word] ?? '.requiresAuthentication';
const available = c => `@available(iOS ${c.placement.offered[0].from}, *)`;

/** The WidgetKit kind of control `kind` of app `appId`. */
export const controlKind = (appId, kind) => `${appId}.control.${kind}`;

/** The intents of `controls`, the same Swift in both targets. */
export function controlIntentsSource(controls) {
  if (!controls.length) return '';
  const out = [`
/// A control's press that did not run: the system shows its message.
struct ExactControlRefusal: Error, CustomLocalizedStringResourceConvertible {
    let message: String
    var localizedStringResource: LocalizedStringResource { "\\(message)" }
}
${controls.some(c => c.control.tool !== undefined) ? `
enum ExactControlPress {
    /// Press control kind, compiled with digest, and await its tool's envelope (LLP 1111 D4). The system's
    /// cancellation of the intent's task is the caller's (ExactTools.swift, Cancellation).
    @MainActor static func run(_ kind: String, digest: String, title: String) async throws {
        #if canImport(ExactToolDoor)
        let cancellation = ExactTools.Cancellation()
        // A task cancelled before it ran calls its handler first, on its own turn: the door refuses at once.
        if Task.isCancelled { cancellation.cancel() }
        let answer: [String: Any] = await withTaskCancellationHandler {
            await withCheckedContinuation { done in
                ExactTools.press(control: kind, digest: digest, cancellation: cancellation) { done.resume(returning: $0) }
            }
        } onCancel: {
            DispatchQueue.main.async { cancellation.cancel() }
        }
        guard answer["outcome"] as? String == "ok" else {
            let message = answer["message"] as? String ?? ""
            throw ExactControlRefusal(message: message.isEmpty ? "\\(title): \\(answer["outcome"] as? String ?? "failed")" : message)
        }
        #else
        throw ExactControlRefusal(message: "\\(title) runs only in the app.")
        #endif
    }
}
` : ''}`];
  for (const c of controls) {
    const { control } = c, T = intentName(c);
    const description = control.description ? `\n    static let description = IntentDescription(${literal(control.description)})` : '';
    if (control.url !== undefined) {
      out.push(`
${available(c)}
enum ${T}_Target: String, AppEnum {
    case location
    static let typeDisplayRepresentation: TypeDisplayRepresentation = ${literal(control.title)}
    static let caseDisplayRepresentations: [Self: DisplayRepresentation] = [.location: ${literal(control.title)}]
}

${available(c)}
struct ${T}: OpenIntent {
    static let title: LocalizedStringResource = ${literal(control.title)}${description}
    static let isDiscoverable = false
    @Parameter(title: ${literal(control.title)}, default: .location) var target: ${T}_Target
    @MainActor func perform() async throws -> some IntentResult {
        NotificationCenter.default.post(name: Notification.Name("exact.contentIndex.open"), object: NSMutableDictionary(dictionary: ["url": ${literal(control.url)}]))
        return .result()
    }
}
`);
      continue;
    }
    out.push(`
${available(c)}
struct ${T}: LiveActivityIntent {
    static let title: LocalizedStringResource = ${literal(control.title)}${description}
    static let isDiscoverable = false
    static let authenticationPolicy: IntentAuthenticationPolicy = ${authentication(control.authentication)}
    @MainActor func perform() async throws -> some IntentResult {
        try await ExactControlPress.run(${literal(control.kind)}, digest: ${literal(c.digest)}, title: ${literal(control.title)})
        return .result()
    }
}
`);
  }
  return out.join('');
}

/** The extension's `ControlWidget` for each of `controls`, with their intents, and the bundle's members. */
export function controlWidgetsSource(appId, controls) {
  const widgets = controls.map(c => {
    const { control } = c, description = control.description ? `.description(${literal(control.description)})` : '';
    return `
${available(c)}
struct Control_${hex(control.kind)}: ControlWidget {
    var body: some ControlWidgetConfiguration {
        StaticControlConfiguration(kind: ${literal(controlKind(appId, control.kind))}) {
            ControlWidgetButton(action: ${intentName(c)}()) {
                Label(${literal(control.title)}, systemImage: ${literal(control.symbol)})
            }
        }
        .displayName(${literal(control.title)})${description}
    }
}
`;
  });
  return {
    source: controlIntentsSource(controls) + widgets.join(''),
    members: controls.map(c => `if #available(iOS ${c.placement.offered[0].from}, *) { Control_${hex(c.control.kind)}() }`),
  };
}
