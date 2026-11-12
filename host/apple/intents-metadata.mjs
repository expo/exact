// App Intents schema 1, compared with Xcode's compile-time extractor. This
// consumes Swift's constants, including its mangled names, for ExactIntents.
const localized = key => ({ alternatives: [], key });
const availability = () => ({ LNPlatformNameWildcard: { introducedVersion: '*' } });
const visibility = () => ({ assistantOnly: false, isDiscoverable: true });
const primitive = typeIdentifier => ({ primitive: { wrapper: { typeIdentifier } } });
const entityValue = typeName => ({ entity: { wrapper: { typeName } } });
const fail = message => { throw new Error(`host/apple: intents metadata: ${message}`); };
const property = (type, name) => type.properties?.find(p => p.label === name && p.isStatic === 'true');
const raw = value => value?.valueKind === 'RawLiteral' && typeof value.value === 'string' ? value.value : fail('expected a constant string');
const argument = (value, label) => value?.value?.arguments?.find(a => a.label === label);
const simple = type => type.typeName.slice('ExactIntents.'.length);
const conforms = (type, name) => type.conformances?.includes(`AppIntents.${name}`);
const identity = type => ({ fullyQualifiedTypeName: type.typeName, mangledTypeName: type.mangledTypeName,
  mangledTypeNameByBundleIdentifier: {}, effectiveBundleIdentifiers: [], availabilityAnnotations: availability(),
  requiredCapabilities: [], typeSpecificMetadata: [], visibilityMetadata: visibility() });

function parameter(p, entities) {
  const optional = /^Swift.Optional<(.+)>$/.exec(p.type), type = optional?.[1] ?? p.type;
  const id = { 'Swift.String': 0, 'Swift.Bool': 1, 'Swift.Int': 2, 'Swift.Double': 7 }[type];
  const entity = type.startsWith('ExactIntents.') ? entities[type.slice('ExactIntents.'.length)] : null;
  if (id == null && !entity) fail(`unsupported parameter type ${p.type}`);
  if (p.valueKind !== 'InitCall' || p.value?.type !== `AppIntents.IntentParameter<${p.type}>`) fail(`missing parameter constants for ${p.label}`);
  const inputTypes = entity ? [] : type === 'Swift.String' ? [primitive(0), { array: { wrapper: { capabilities: 3, memberValueType: primitive(2) } } }, primitive(2)]
    : (type === 'Swift.Bool' ? [1] : type === 'Swift.Double' ? [2, 0, 7] : optional ? [7, 0, 2] : [0, 2, 7]).map(primitive);
  return { capabilities: 0, dynamicOptionsSupport: entity ? 2 : 0, inputConnectionBehavior: 0, isInput: false, isOptional: !!optional,
    name: p.label, resolvableInputTypes: inputTypes.map(valueType => ({ kindValue: 0, valueType })),
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
export function intentsMetadata(types, sdkMajor = 27) {
  if (!Array.isArray(types)) fail('expected Swift constant values');
  if (![26, 27].includes(sdkMajor)) fail(`unqualified SDK generation ${sdkMajor}`);
  const names = new Set();
  for (const type of types) {
    if (!type.typeName?.startsWith('ExactIntents.') || !type.mangledTypeName || names.has(type.typeName)) fail('invalid or duplicate Swift type');
    names.add(type.typeName);
    if (!['AppIntent', 'AppEntity', 'EntityStringQuery', 'AppShortcutsProvider'].some(name => conforms(type, name))) fail(`unsupported conformer ${type.typeName}`);
  }
  const entities = {}, queries = {}, actions = {}, autoShortcuts = [];
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
    const parameters = (type.properties ?? []).filter(p => p.isStatic === 'false' && p.isComputed === 'true' && !p.label.startsWith('$') && p.valueKind === 'InitCall' && p.value?.type?.startsWith('AppIntents.IntentParameter<')).map(p => parameter(p, entities));
    // Every emitted @Parameter has one backing wrapper; none may be lost.
    if (parameters.length !== (type.properties ?? []).filter(p => p.isComputed === 'false' && p.label.startsWith('_') && p.value?.type?.startsWith('AppIntents.IntentParameter<')).length) fail(`incomplete parameters for ${type.typeName}`);
    const system = open ? ['com.apple.link.systemProtocol.OpenEntity'] : [];
    const systemMetadata = open ? [...system, { empty: {} }] : [];
    const description = property(type, 'description');
    actions[simple(type)] = { ...identity(type), identifier: simple(type), mangledTypeNameV2: type.mangledTypeName, mangledTypeNameByBundleIdentifierV2: {},
      title: localized(raw(property(type, 'title'))), ...(description ? { descriptionMetadata: { descriptionText: localized(raw(description)), searchKeywords: [] } } : {}),
      assistantDefinedSchemaTraits: [], assistantDefinedSchemas: [], authenticationPolicy: auth, isAuthPolExplicit: !!policy, isDiscoverable: true,
      openAppWhenRun: open, outputFlags: 0, parameters, presentationStyle: 0, supportedModes: open ? 2 : 1,
      systemProtocols: system, systemProtocolMetadata: systemMetadata, systemProtocolMetadataV2: systemMetadata };
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
      const intent = argument(value, 'intent')?.type?.slice('ExactIntents.'.length), phrases = argument(value, 'phrases');
      if (!actions[intent] || phrases?.valueKind !== 'Array') fail('invalid shortcut intent or phrases');
      autoShortcuts.push({ actionIdentifier: intent, availabilityAnnotations: availability(), phraseTemplates: phrases.value.map(p => shortcutPhrase(p, actions[intent])),
        shortTitle: localized(raw(argument(value, 'shortTitle'))), systemImageName: raw(argument(value, 'systemImageName')) });
    }
  }
  return { actions, assistantEntities: [], assistantIntentNegativePhrases: [], assistantIntents: [],
    ...(providers.length ? { autoShortcutProviderMangledName: providers[0].mangledTypeName } : {}), autoShortcuts, entities, enums: [],
    generator: { name: 'exact', version: '1' }, negativePhrases: [], queries, shortcutTileColor: 14, version: 1 };
}
