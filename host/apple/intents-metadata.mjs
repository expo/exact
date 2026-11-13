// App Intents schema 1, compared with Xcode's compile-time extractor. This
// consumes Swift's constants, including its mangled names, for ExactIntents,
// and for a widget extension's module (ExactWidgets, or the alarm's): its
// WidgetConfigurationIntents and their String AppEnum parameters.
// The widget shapes are compared with Xcode 26.4.1's extractor (SDK 26) by
// intents-metadata.test.mjs, for iOS and watchOS.
let prefix = 'ExactIntents.';
const localized = key => ({ alternatives: [], key });
const availability = () => ({ LNPlatformNameWildcard: { introducedVersion: '*' } });
const visibility = () => ({ assistantOnly: false, isDiscoverable: true });
const primitive = typeIdentifier => ({ primitive: { wrapper: { typeIdentifier } } });
const entityValue = typeName => ({ entity: { wrapper: { typeName } } });
const fail = message => { throw new Error(`host/apple: intents metadata: ${message}`); };
const property = (type, name) => type.properties?.find(p => p.label === name && p.isStatic === 'true');
const raw = value => value?.valueKind === 'RawLiteral' && typeof value.value === 'string' ? value.value : fail('expected a constant string');
const argument = (value, label) => value?.value?.arguments?.find(a => a.label === label);
const simple = type => type.typeName.slice(prefix.length);
const conforms = (type, name) => type.conformances?.includes(`AppIntents.${name}`);
const identity = type => ({ fullyQualifiedTypeName: type.typeName, mangledTypeName: type.mangledTypeName,
  mangledTypeNameByBundleIdentifier: {}, effectiveBundleIdentifiers: [], availabilityAnnotations: availability(),
  requiredCapabilities: [], typeSpecificMetadata: [], visibilityMetadata: visibility() });

// What a WidgetConfigurationIntent's action changes in an AppIntent's.
const WIDGET_PROTOCOL = 'com.apple.link.systemProtocol.WidgetConfiguration';

function parameter(p, entities, enums = {}) {
  const optional = /^Swift.Optional<(.+)>$/.exec(p.type), type = optional?.[1] ?? p.type;
  const id = { 'Swift.String': 0, 'Swift.Bool': 1, 'Swift.Int': 2, 'Swift.Double': 7 }[type];
  const own = type.startsWith(prefix) ? type.slice(prefix.length) : null;
  const entity = own ? entities[own] : null, enumeration = own ? enums[own] : null;
  if (enumeration) {
    if (p.valueKind !== 'InitCall' || p.value?.type !== `AppIntents.IntentParameter<${p.type}>`) fail(`missing parameter constants for ${p.label}`);
    // widgets.mjs gives every row a default (the manifest's, or its first value); Xcode records its raw value.
    const fallback = argument(p, 'default'), rawDefault = fallback?.valueKind === 'Enum' ? enumeration.rawValues[fallback.value?.name] : undefined;
    if (rawDefault == null) fail(`invalid default for ${p.label}`);
    return { capabilities: 1, dynamicOptionsSupport: 0, inputConnectionBehavior: 0, isInput: false, isOptional: !!optional,
      name: p.label, resolvableInputTypes: [], title: localized(raw(argument(p, 'title'))),
      typeSpecificMetadata: ['LNValueTypeSpecificMetadataKeyDefaultValue', { string: { wrapper: rawDefault } }],
      valueType: { linkEnumeration: { wrapper: { identifier: own } } } };
  }
  if (id == null && !entity) fail(`unsupported parameter type ${p.type}`);
  if (p.valueKind !== 'InitCall' || p.value?.type !== `AppIntents.IntentParameter<${p.type}>`) fail(`missing parameter constants for ${p.label}`);
  const inputTypes = entity ? [] : type === 'Swift.String' ? [primitive(0), { array: { wrapper: { capabilities: 3, memberValueType: primitive(2) } } }, primitive(2)]
    : (type === 'Swift.Bool' ? [1] : type === 'Swift.Double' ? [2, 0, 7] : optional ? [7, 0, 2] : [0, 2, 7]).map(primitive);
  const description = argument(p, 'description'), described = description && description.valueKind !== 'NilLiteral';
  return { capabilities: 0, dynamicOptionsSupport: entity ? 2 : 0, inputConnectionBehavior: 0, isInput: false, isOptional: !!optional,
    name: p.label, ...(described ? { parameterDescription: localized(raw(description)) } : {}),
    resolvableInputTypes: inputTypes.map(valueType => ({ kindValue: 0, valueType })),
    title: localized(raw(argument(p, 'title'))), typeSpecificMetadata: [], valueType: entity ? entityValue(entity.typeName) : primitive(id) };
}

function shortcutPhrase(value, action) {
  if (value.valueKind !== 'InterpolatedStringLiteral') fail('expected an interpolated shortcut phrase');
  return localized(value.value.segments.map(segment => {
    if (segment.valueKind === 'RawLiteral') return raw(segment);
    if (segment.valueKind === 'Enum' && segment.value?.name === 'applicationName') return '${applicationName}';
    if (segment.valueKind === 'KeyPath') {
      const path = segment.value, name = path?.path?.slice(1);
      if (path?.rootType !== action.fullyQualifiedTypeName || !path.path.startsWith('$') || path.components?.length !== 1
        || path.components[0].label !== path.path || !action.parameters.some(p => p.name === name)) fail('invalid shortcut parameter reference');
      return '${' + name + '}';
    }
    fail(`unsupported shortcut interpolation ${segment.valueKind}`);
  }).join(''));
}

/** Only the Swift shapes intentsSource emits are accepted; unknown shapes fail the build. */
export function intentsMetadata(types, sdkMajor = 27, { module = 'ExactIntents' } = {}) {
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(module)) fail(`invalid module name ${module}`);
  prefix = `${module}.`;
  if (!Array.isArray(types)) fail('expected Swift constant values');
  if (![26, 27].includes(sdkMajor)) fail(`unqualified SDK generation ${sdkMajor}`);
  const names = new Set();
  for (const type of types) {
    if (!type.typeName?.startsWith(prefix) || !type.mangledTypeName || names.has(type.typeName)) fail('invalid or duplicate Swift type');
    names.add(type.typeName);
    if (!['AppIntent', 'AppEntity', 'EntityStringQuery', 'AppShortcutsProvider', 'AppEnum'].some(name => conforms(type, name))) fail(`unsupported conformer ${type.typeName}`);
  }
  const entities = {}, queries = {}, actions = {}, autoShortcuts = [], enums = {};
  // A String AppEnum (widgets.mjs's configuration rows): its cases by raw value, each with its constant title.
  for (const type of types.filter(type => conforms(type, 'AppEnum'))) {
    const rawType = type.associatedTypeAliases?.find(a => a.typeAliasName === 'RawValue')?.substitutedTypeName;
    if (type.kind !== 'enum' || rawType !== 'Swift.String' || !type.cases?.length) fail(`unsupported AppEnum ${type.typeName}`);
    const shown = property(type, 'caseDisplayRepresentations');
    if (shown?.valueKind !== 'Dictionary') fail(`missing case titles for ${type.typeName}`);
    const titles = new Map(shown.value.map(({ key, value }) => [key?.valueKind === 'Enum' ? key.value?.name : fail(`invalid case title key for ${type.typeName}`), raw(value)]));
    if (titles.size !== type.cases.length || type.cases.some(c => !titles.has(c.name) || typeof c.rawValue !== 'string')) fail(`incomplete case titles for ${type.typeName}`);
    const { requiredCapabilities, typeSpecificMetadata, ...common } = identity(type);
    enums[simple(type)] = { ...common, identifier: simple(type), displayTypeName: localized(raw(property(type, 'typeDisplayRepresentation'))),
      cases: type.cases.map(c => ({ identifier: c.rawValue, displayRepresentation: { title: localized(titles.get(c.name)) } })),
      assistantDefinedSchemas: [], isSystem: false, rawValues: Object.fromEntries(type.cases.map(c => [c.name, c.rawValue])) };
  }
  for (const type of types.filter(type => conforms(type, 'AppEntity'))) {
    const query = property(type, 'defaultQuery')?.type;
    if (!types.some(t => t.typeName === query && conforms(t, 'EntityStringQuery'))) fail(`missing entity query for ${type.typeName}`);
    entities[simple(type)] = { ...identity(type), typeName: simple(type), displayTypeName: localized(raw(property(type, 'typeDisplayRepresentation'))),
      defaultQueryIdentifier: query, entitySyncMetadata: { entitySyncType: 0 }, properties: [], systemProtocolMetadata: [], systemProtocolMetadataV2: [],
      assistantDefinedSchemas: [], transient: false };
    if (sdkMajor === 26) { delete entities[simple(type)].entitySyncMetadata; delete entities[simple(type)].typeSpecificMetadata; }
  }
  for (const type of types.filter(type => conforms(type, 'EntityStringQuery'))) {
    const entity = Object.values(entities).find(e => e.defaultQueryIdentifier === type.typeName);
    if (!entity) fail(`unowned query ${type.typeName}`);
    queries[simple(type)] = { availabilityAnnotations: availability(), capabilities: 70, defaultQueryForEntity: true, effectiveBundleIdentifiers: [],
      entityType: entity.typeName, fullyQualifiedIdentifier: type.typeName, identifier: simple(type), mangledTypeName: type.mangledTypeName,
      mangledTypeNameByBundleIdentifier: {}, parameters: [], queryType: simple(type), resultValueType: entityValue(entity.typeName), sortingOptions: [], visibilityMetadata: visibility() };
  }
  for (const type of types.filter(type => conforms(type, 'AppIntent'))) {
    const open = conforms(type, 'OpenIntent'), policy = property(type, 'authenticationPolicy');
    const auth = policy ? { alwaysAllowed: 0, requiresAuthentication: 1, requiresLocalDeviceAuthentication: 2 }[policy.value?.name] : 0;
    if (auth == null || (policy && policy.valueKind !== 'Enum')) fail(`unsupported authentication policy for ${type.typeName}`);
    const widget = conforms(type, 'WidgetConfigurationIntent');
    const parameters = (type.properties ?? []).filter(p => p.isStatic === 'false' && p.isComputed === 'true' && !p.label.startsWith('$') && p.valueKind === 'InitCall' && p.value?.type?.startsWith('AppIntents.IntentParameter<')).map(p => parameter(p, entities, widget ? enums : {}));
    // Every emitted @Parameter has one backing wrapper; none may be lost.
    if (parameters.length !== (type.properties ?? []).filter(p => p.isComputed === 'false' && p.label.startsWith('_') && p.value?.type?.startsWith('AppIntents.IntentParameter<')).length) fail(`incomplete parameters for ${type.typeName}`);
    const system = open ? ['com.apple.link.systemProtocol.OpenEntity'] : widget ? [WIDGET_PROTOCOL] : [];
    const systemMetadata = system.length ? [...system, { empty: {} }] : [];
    const description = property(type, 'description');
    actions[simple(type)] = { ...identity(type), identifier: simple(type), mangledTypeNameV2: type.mangledTypeName, mangledTypeNameByBundleIdentifierV2: {},
      title: localized(raw(property(type, 'title'))), ...(description ? { descriptionMetadata: { descriptionText: localized(raw(description)), searchKeywords: [] } } : {}),
      assistantDefinedSchemaTraits: [], assistantDefinedSchemas: [], authenticationPolicy: auth, isAuthPolExplicit: !!policy, isDiscoverable: !widget,
      openAppWhenRun: open, outputFlags: widget ? 8 : 0, parameters, presentationStyle: 0, supportedModes: open ? 2 : 1,
      systemProtocols: system, systemProtocolMetadata: systemMetadata, systemProtocolMetadataV2: systemMetadata };
    if (widget && open) fail(`unsupported widget OpenIntent ${type.typeName}`);
    if (open) {
      const entity = entities[parameters[0]?.valueType?.entity?.wrapper?.typeName];
      if (!entity || parameters.length !== 1 || parameters[0].name !== 'target') fail(`unsupported OpenIntent ${type.typeName}`);
      actions[simple(type)].typeSpecificMetadata = ['LNActionTypeMetadataKeyEntityType', { string: { wrapper: entity.typeName } },
        'LNActionTypeMetadataKeyEntityMetadata', { entityMetadata: { wrapper: entity } }];
    }
  }
  const providers = types.filter(type => conforms(type, 'AppShortcutsProvider'));
  if (providers.length > 1) fail('more than one shortcuts provider');
  if (providers.length) {
    const builder = property(providers[0], 'appShortcuts');
    if (builder?.valueKind !== 'Builder' || builder.value?.type !== 'AppIntents.AppShortcutsBuilder') fail('missing shortcuts builder');
    for (const member of builder.value.members) {
      const value = member.element;
      if (member.kind !== 'buildExpression' || value?.valueKind !== 'InitCall' || value.value?.type !== 'AppIntents.AppShortcut') fail('unsupported shortcut expression');
      const intent = argument(value, 'intent')?.type?.slice(prefix.length), phrases = argument(value, 'phrases');
      if (!actions[intent] || phrases?.valueKind !== 'Array') fail('invalid shortcut intent or phrases');
      autoShortcuts.push({ actionIdentifier: intent, availabilityAnnotations: availability(), phraseTemplates: phrases.value.map(p => shortcutPhrase(p, actions[intent])),
        shortTitle: localized(raw(argument(value, 'shortTitle'))), systemImageName: raw(argument(value, 'systemImageName')) });
    }
  }
  return { actions, assistantEntities: [], assistantIntentNegativePhrases: [], assistantIntents: [],
    ...(providers.length ? { autoShortcutProviderMangledName: providers[0].mangledTypeName } : {}), autoShortcuts, entities,
    enums: Object.values(enums).map(({ rawValues, ...e }) => e),
    generator: { name: 'exact', version: '1' }, negativePhrases: [], queries, shortcutTileColor: 14, version: 1 };
}
