// @ref LLP 1172.001 E4, §4 step 4 — today's Apple JSON wire, with one declaration.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const codecs = {
  inlineStyle: ['InlineStyle', 'InlineStyle()', 'inlineStyle()'],
  adapterRect: ['CGRect?', 'nil', 'value().adapterRect'],
  adapterNumberRect: ['CGRect?', 'nil', 'value().adapterNumberRect'],
  adapterPadding: ['[CGFloat?]?', 'nil', 'value().adapterPadding'],
  adapterID: ['UInt32?', 'nil', 'value().adapterID'],
  adapterIDs: ['[UInt32]?', 'nil', 'value().adapterIDs'],
  adapterRank: ['Int64?', 'nil', 'value().adapterRank'],
  adapterString: ['String?', 'nil', 'value().adapterString'],
  adapterBool: ['Bool?', 'nil', 'value().adapterBool'],
  string: ['String', '""', 'string()'],
  number: ['Double', '0', 'number()'],
  optionalNumber: ['Double?', 'nil', 'number()'],
  props: ['[String: String]', '[:]', 'strings()'],
  style: ['NodeStyle', '[:]', 'style()'],
  values: ['NodeStyle', '[:]', 'values()'],
  strings: ['[String]', '[]', 'array { try $0.string() }'],
  handlers: ['Set<String>', '[]', 'Set(try array { try $0.string() })'],
  ids: ['[UInt32]', '[]', 'array { try $0.id() }'],
  inline: ['[InlineText]', '[]', 'array { try $0.inline() }'],
  bool: ['Bool', 'false', 'bool()'],
  optionalBool: ['Bool?', 'nil', 'bool()'],
  optionalString: ['String?', 'nil', 'string()'],
  id: ['UInt32', '0', 'id()'],
  uint64String: ['UInt64', '0', null],
  ops: ['[BatchOp]', '[]', 'array { try $0.batchOp() }'],
  numbers: ['[Double]', '[]', 'array { try $0.number() }'],
  nativeTimers: ['[NativeTimerWake]?', 'nil', 'array { try $0.nativeTimer() }'],
  modes: ['[String: String]?', 'nil', 'batchModes()'],
  stringValue: ['String?', 'nil', 'value().string'],
  orientation: ['OrientationRequest?', 'nil', 'batchOrientation()'],
  routeInspectors: ['[RouteInspector]?', 'nil', 'batchRouteInspectors()'],
  rootTabs: ['[RootTabs]?', 'nil', 'batchRootTabs()'],
  sequence: ['(UInt64, UInt64)?', 'nil', 'batchSequence()'],
  collectionSnapshots: ['[CollectionSnapshot]?', 'nil', 'linkedCollectionSnapshots()'],
};
const identifier = name => name.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
const quoted = JSON.stringify;

function envelopeReader(record) {
  const fields = Object.entries(record.fields);
  const lines = ['extension BatchReader {', '    package mutating func batch() throws -> Batch {'];
  for (const [, field] of fields) {
    const [type, empty] = codecs[field.codec];
    lines.push(`        var ${field.member}: ${type} = ${empty}`);
  }
  lines.push('        var seen: Set<String> = []', '        try object { r, key in',
    '            guard seen.insert(key).inserted else { try r.skip(); return }',
    '            if try r.null() { return }', '            switch key {');
  for (const [key, field] of fields) lines.push(`            case ${quoted(key)}: ${field.member} = try r.${codecs[field.codec][2]}`);
  lines.push('            default: try r.skip()', '            }', '        }');
  const args = record.initializer.map(name => `${name}: ${name}`);
  lines.push(`        var batch = Batch(${args.join(', ')})`);
  for (const [, field] of fields.filter(([, f]) => !record.initializer.includes(f.member))) lines.push(`        batch.${field.member} = ${field.member}`);
  lines.push('        return batch', '    }', '}', '');
  return lines.join('\n');
}

function nativeTimerReader(record) {
  const fields = Object.entries(record.fields);
  const args = fields.map(([name]) => `${name}: ${name}`).join(', ');
  const lines = ['/// One ordinary task\'s native timer; generations reject delivery after cancellation.',
    'public struct NativeTimerWake: Equatable, Sendable {'];
  for (const [name, field] of fields) lines.push(`    public let ${name}: ${codecs[field.codec][0]}`);
  lines.push(`    public init(${fields.map(([n, f]) => `${n}: ${codecs[f.codec][0]}`).join(', ')}) {`);
  for (const [name] of fields) lines.push(`        self.${name} = ${name}`);
  lines.push('    }', '}', '', 'extension BatchReader {',
    '    package mutating func nativeTimer() throws -> NativeTimerWake {');
  for (const [name, field] of fields) lines.push(`        var ${name}: ${codecs[field.codec][0]}?`);
  lines.push('        try object { r, key in', '            switch key {');
  for (const [name, field] of fields) {
    const read = field.codec === 'uint64String' ? 'UInt64(try r.string())' : `try r.${codecs[field.codec][2]}`;
    lines.push(`            case ${quoted(name)}: ${name} = ${read}`);
  }
  lines.push('            default: try r.skip()', '            }', '        }');
  const guards = fields.flatMap(([name, field]) => [`let ${name}`,
    ...(field.minimum === undefined ? [] : [`${name} >= ${field.minimum}`]),
    ...(field.exclusiveMinimum === undefined ? [] : [`${name} > ${field.exclusiveMinimum}`])]);
  lines.push(`        guard ${guards.join(', ')} else { throw Invalid.wire }`,
    `        return NativeTimerWake(${args})`, '    }', '}', '');
  return lines.join('\n');
}

function inlineReaders(record, rows) {
  const fields = Object.entries(record.fields);
  const lines = ['extension BatchReader {', '    package mutating func inline() throws -> InlineText {'];
  for (const [, field] of fields) {
    const [type, empty] = codecs[field.codec];
    lines.push(`        var ${field.member}: ${type}${field.required ? '?' : ''} = ${field.required ? 'nil' : field.default ?? empty}`);
  }
  lines.push('        try object { r, key in', '            switch key {');
  for (const [key, field] of fields.filter(([, f]) => f.required)) lines.push(`            case ${quoted(key)}: ${field.member} = try r.${codecs[field.codec][2]}`);
  lines.push('            default:', '                if try r.null() { return }', '                switch key {');
  for (const [key, field] of fields.filter(([, f]) => !f.required)) lines.push(`                case ${quoted(key)}: ${field.member} = ${field.codec === 'handlers' ? 'Set(try r.array { try $0.string() })' : 'try r.' + codecs[field.codec][2]}`);
  lines.push('                default: try r.skip()', '                }', '            }', '        }',
    `        guard ${fields.filter(([, f]) => f.required).map(([, f]) => 'let ' + f.member).join(', ')} else { throw Invalid.wire }`,
    `        return InlineText(${fields.map(([, f]) => `${f.member}: ${f.member}`).join(', ')})`, '    }');
  const height = rows.find(r => r.appleInline?.read === 'height').field;
  lines.push('    package mutating func inlineStyle() throws -> InlineStyle {',
    '        var style = InlineStyle(), height: BatchValue?', '        try object { r, key in',
    `            if key == ${quoted(height)} { height = try r.value() }`,
    '            else { try style.set(key, r.value(), colorChannels: r.colorChannels) }',
    '        }', '        try style.height(height)', '        return style', '    }', '}', '',
    'extension BatchFields {',
    '    package func inline(colorChannels: ((BatchValue, Bool) -> [Double]?)? = nil) throws -> InlineText {',
    `        guard ${fields.filter(([, f]) => f.required).map(([k, f]) => `let ${f.member} = try self.id(${quoted(k)})`).join(', ')} else { throw BatchReader.Invalid.wire }`,
    '        var style = InlineStyle()', '        let rows = try object("style")',
    `        for (key, value) in rows where key != ${quoted(height)} { try style.set(key, value, colorChannels: colorChannels) }`,
    `        try style.height(rows[${quoted(height)}])`, '        return InlineText(');
  const reads = { props: 'strings', handlers: 'handlers', bool: 'bool', number: 'number' };
  lines.push('            ' + fields.map(([key, f]) => {
    if (f.required || f.codec === 'inlineStyle') return `${f.member}: ${f.member}`;
    const q = quoted(key);
    const read = f.codec === 'handlers' ? `Set(try array(${q}, Self.string))` : `try ${reads[f.codec]}(${q})${['bool', 'number'].includes(f.codec) ? ' ?? ' + (f.default ?? codecs[f.codec][1]) : ''}`;
    return `${f.member}: ${read}`;
  }).join(', ') + ')', '    }', '}', '');
  return lines.join('\n');
}

function inlineStyleDispatch(rows) {
  const lines = ['extension InlineStyle {',
    '    package mutating func set(_ key: String, _ value: BatchValue, colorChannels: ((BatchValue, Bool) -> [Double]?)? = nil) throws {',
    '        if value == .null { return }', '        switch key {'];
  for (const row of rows.filter(r => r.appleInline && r.appleInline.read !== 'height')) {
    const { read, member, word, negated, foreground, index } = row.appleInline;
    const expressions = {
      float: `${member} = CGFloat(Float(try BatchFields.number(value)))`,
      integer: `let n = try BatchFields.number(value)\n            guard n >= Double(Int.min), n < Double(Int.max) else { throw BatchReader.Invalid.wire }\n            ${member} = Int(n)`,
      byte: `${member} = Int(try BatchFields.number(value)) & 0xff`,
      string: `${member} = try BatchFields.string(value)`,
      keyword: `${member} = try BatchFields.string(value) ${negated ? '!=' : '=='} ${quoted(word)}`,
      variations: 'run.variations = FontAxis.settings(value, keeping: run.variations)',
      opticalSizing: 'run.variations = FontAxis.opticalSizing(try BatchFields.string(value), run.variations)',
      features: 'run.features = FontFeature.of(value)',
      color: `try color(value, foreground: ${foreground}, channels: colorChannels ?? { $0.fallbackChannels(dark: $1) })`,
      borderColor: `borderColors[${index}] = value\n            if value.isSchemeColor { paired = true }`,
      borderStyle: `borderStyles[${index}] = try BatchFields.string(value)`,
      radius: `if run.backgroundRadii.isEmpty { run.backgroundRadii = Array(repeating: InlineRadius(), count: 4) }\n            run.backgroundRadii[${index}] = InlineRadius(value)`,
    };
    if (!expressions[read]) throw new Error(`unknown inline style reader: ${read}`);
    lines.push(`        case ${quoted(row.field)}: ${expressions[read]}`);
  }
  lines.push('        default: paint.set(key, value)', '        }', '    }', '}', '');
  return lines.join('\n');
}

function runPaintDispatch(rows) {
  const paint = rows.filter(r => r.appleRunPaint);
  const lines = ['extension RunPaintRows {', '    package init(_ style: NodeStyle) {', '        self.init()'];
  for (const row of paint) lines.push(`        if let value = style[${quoted(row.field)}] { set(${quoted(row.field)}, value) }`);
  lines.push('    }', '    @discardableResult', '    package mutating func set(_ key: String, _ value: BatchValue) -> Bool {', '        switch key {');
  const expressions = { shadow: 'readShadow(value)', strokeWidth: 'strokeWidth = value.number ?? 0', strokeColor: 'strokeColor = value.string == nil ? value : nil' };
  for (const row of paint) {
    if (!expressions[row.appleRunPaint]) throw new Error(`unknown run paint reader: ${row.appleRunPaint}`);
    lines.push(`        case ${quoted(row.field)}: ${expressions[row.appleRunPaint]}`);
  }
  lines.push('        default: return false', '        }', '        return true', '    }', '}', '');
  return lines.join('\n');
}

function arrayReaders(arrays) {
  const lines = [];
  for (const array of arrays) {
    const fields = Object.entries(array.fields);
    lines.push(`package struct ${array.type}: Equatable {`);
    for (const [name, field] of fields) {
      const type = codecs[field.codec][0];
      lines.push(`    package let ${name}: ${field.required ? type.replace(/\?$/, '') : type}`);
    }
    lines.push('}', '', 'extension BatchValue {',
      `    package var ${array.codec}: [${array.type}]? {`,
      `        guard let rows = self.array, rows.allSatisfy({ $0.array != nil })${array.nonempty ? ', !rows.isEmpty' : ''} else { return nil }`,
      '        return rows.compactMap { row in',
      `            guard let values = row.array, values.count ${array.count ? '== ' + array.count : '>= ' + array.minimum} else { return nil }`);
    for (const [name, field] of fields) {
      const value = `values[${field.index}].${field.codec}`;
      lines.push(field.required ? `            guard let ${name} = ${value} else { return nil }`
        : `            let ${name} = values.count > ${field.index} ? ${value} : nil`);
    }
    lines.push(`            return ${array.type}(${fields.map(([name]) => `${name}: ${name}`).join(', ')})`,
      '        }', '    }', '}', '');
  }
  return lines.join('\n');
}

export function appleBatchReader(schema) {
  const records = schema.appleBatch.records;
  const arrays = schema.appleBatch.arrays ?? [];
  const readers = { ...codecs, ...Object.fromEntries(arrays.map(a => [a.codec, [`[${a.type}]?`, 'nil', `value().${a.codec}`]])) };
  const members = new Map();
  for (const record of records) for (const field of Object.values(record.fields ?? {})) {
    if (!readers[field.codec]) throw new Error(`unknown Apple batch codec: ${field.codec}`);
    const [type, empty] = readers[field.codec];
    const declaration = `${type} = ${field.default === undefined ? empty : quoted(field.default)}`;
    if (members.has(field.member) && members.get(field.member) !== declaration) throw new Error(`conflicting Apple batch member: ${field.member}`);
    members.set(field.member, declaration);
  }
  const lines = [
    '// Generated from kernel/tables/schema.json by kernel/build/apple-batch.mjs. Do not edit.',
    '// @ref LLP 1172.001 E4, §4 step 4',
    'import Foundation', '', 'public struct BatchOp {',
    '    package enum Kind: String {',
    ...records.map(record => `        case ${identifier(record.name)} = ${quoted(record.name)}`),
    '        case unknown', '    }',
    '    package let op: Kind',
    '    package var nodeID: UInt32?',
    '    package var id: UInt32 { nodeID ?? 0 }',
    ...[...members].map(([name, declaration]) => `    package var ${name}: ${declaration}`),
    '    package var payload: [String: Any] = [:]',
    '    /// Prepared paint belongs to the owner, outside the wire.',
    '    package var flat: FlatPaint?',
    '    package init(op: Kind, nodeID: UInt32? = nil) { self.op = op; self.nodeID = nodeID }',
    '    package var isTypedPayload: Bool {', '        switch op {',
    `        case ${records.filter(r => r.typedPayload).map(r => '.' + identifier(r.name)).join(', ')}: return true`,
    '        default: return false', '        }', '    }',
    '    package var isAdapter: Bool {', '        switch op {',
    `        case ${records.filter(r => r.fields).map(r => '.' + identifier(r.name)).join(', ')}: return false`,
    '        default: return true', '        }', '    }', '}', '',
    'extension BatchReader {',
    '    package mutating func recordField(_ op: inout BatchOp, key: String) throws {',
    '        switch (op.op, key) {',
    `        case ${records.filter(r => r.typedPayload).map(r => `(.${identifier(r.name)}, \"id\")`).join(', ')}: op.nodeID = try null() ? nil : id()`,
    '        case (_, "id"): op.nodeID = try id()',
  ];
  for (const record of records) for (const [key, field] of Object.entries(record.fields ?? {})) {
    lines.push(`        case (.${identifier(record.name)}, ${quoted(key)}): op.${field.member} = ${field.codec === 'handlers' ? '' : 'try '}${readers[field.codec][2]}`);
  }
  lines.push('        default: if op.isTypedPayload { _ = try value() } else { try skip() }', '        }', '    }', '}', '',
    'extension BatchFields {',
    '    package func op(colorChannels: ((BatchValue, Bool) -> [Double]?)? = nil) throws -> BatchOp {',
    '        guard let name = try string("op") else { throw BatchReader.Invalid.wire }',
    '        var op = BatchOp(op: BatchOp.Kind(rawValue: name) ?? .unknown, nodeID: try id("id"))',
    '        switch op.op {');
  for (const record of records.filter(r => r.fields)) {
    lines.push(`        case .${identifier(record.name)}:`);
    if (!Object.keys(record.fields).length) lines.push('            break');
    // Streaming snapshots are read before the out-of-order fields fallback.
    if (Object.values(record.fields).some(field => field.codec === 'collectionSnapshots')) {
      lines.push('            throw BatchReader.Invalid.wire');
      continue;
    }
    for (const [key, field] of Object.entries(record.fields)) {
      const q = quoted(key);
      const reads = {
        string: `string(${q}) ?? ${quoted(field.default ?? '')}`,
        number: `number(${q}) ?? ${quoted(field.default ?? 0)}`, optionalNumber: `number(${q})`,
        props: `strings(${q})`, style: `CalculatedLength.decode(object(${q}))`, values: `object(${q})`,
        strings: `array(${q}, Self.string)`, handlers: `Set(try array(${q}, Self.string))`,
        ...Object.fromEntries([...Object.keys(codecs).filter(c => c.startsWith('adapter')), ...arrays.map(a => a.codec)].map(c => [c, `value(${q})?.${c}`])),
        ids: `array(${q}, Self.id)`, inline: `array(${q}) { try Self(Self.object($0)).inline(colorChannels: colorChannels) }`,
      };
      lines.push(`            op.${field.member} = ${field.codec === 'handlers' || field.codec.startsWith('adapter') || arrays.some(a => a.codec === field.codec) ? '' : 'try '}${reads[field.codec]}`);
    }
  }
  lines.push('        default: op.payload = fields.mapValues(\\.any)',
    '        }', '        return op', '    }', '}', '');
  return lines.join('\n') + '\n' + arrayReaders(arrays) + '\n' + envelopeReader(schema.appleBatch.envelope) + '\n' + nativeTimerReader(schema.appleBatch.nativeTimer) + '\n' + inlineReaders(schema.appleBatch.inline, schema.styles) + '\n' + inlineStyleDispatch(schema.styles) + '\n' + runPaintDispatch(schema.styles);
}

export function writeAppleBatchReader() {
  const schema = JSON.parse(readFileSync(new URL('../tables/schema.json', import.meta.url), 'utf8'));
  const output = fileURLToPath(new URL('../../host/apple/Sources/ExactCore/GeneratedBatch.swift', import.meta.url));
  const source = appleBatchReader(schema);
  if (!existsSync(output) || readFileSync(output, 'utf8') !== source) writeFileSync(output, source);
}

if (import.meta.main) writeAppleBatchReader();
