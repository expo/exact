// What package code, and the app's own JavaScript, may use where a data
// module runs (LLP 1027 D5, "Package code"): the check `typescript.mjs` runs on
// every JavaScript file the capture bundles, and the syntax helpers it shares.
// The native bake writes it beside that file in its stage (js/bake/src/resident.rs `PORTABLE`).

// `x!`, `(x)` and `x?.y` as written, down to the expression they wrap.
export const bare=(node)=>{ while (node && ['TSNonNullExpression','ParenthesizedExpression','ChainExpression','TSAsExpression','TSSatisfiesExpression','TSTypeAssertion'].includes(node.type)) node=node.expression; return node; };
// Literal bracket keys, destructuring keys and JSX members name the same properties as dot access.
// Dynamic keys and aliases stay with the runtime guard; do not evaluate them here.
export const propertyName=(node)=>{
  if (!['MemberExpression','JSXMemberExpression','Property'].includes(node?.type)) return null;
  const property=bare(node.type==='Property' ? node.key : node.property);
  if (!node.computed && ['Identifier','JSXIdentifier'].includes(property?.type)) return property.name;
  if (property?.type==='Literal' && typeof property.value==='string') return property.value;
  if (property?.type==='TemplateLiteral' && !property.expressions.length) return property.quasis[0].value.cooked;
  return null;
};
// `line:column` of an offset in `code`, by ECMAScript's line terminators:
// LF, CR (CRLF is one), U+2028, U+2029.
export function lineColumn(code) {
  const starts=[0];
  for (let i=0;i<code.length;i++) {
    const c=code.charCodeAt(i);
    if (c===10 || c===0x2028 || c===0x2029 || (c===13 && code.charCodeAt(i+1)!==10)) starts.push(i+1);
  }
  return (offset)=>{ let lo=0, hi=starts.length; while (lo+1<hi) { const mid=(lo+hi)>>1; if (starts[mid]<=offset) lo=mid; else hi=mid; } return (lo+1)+':'+(offset-starts[lo]+1); };
}
// What package code, and the app's own JavaScript, may not use: the DOM,
// which no data module has, Node's globals, and code made from a string.
// Both executors lack the first two alike (js/src/prelude.js defines none;
// host/web-js/ts-fetch.js hides the page's), so a test of one (`typeof
// window`) answers the same everywhere; a use that test does not guard is
// refused here, by name. This keeps a module portable; it is no security
// boundary (LLP 1027 D5): on the web the module runs in the page's realm.
const DOM=['window','frames','parent','top','opener','document','navigator','location','history','localStorage','sessionStorage','indexedDB','HTMLElement','Element','getComputedStyle','customElements','alert'];
// The global object's names for itself, through which a member reaches the page.
const HOSTS=['globalThis','self','window','frames','parent','top','opener'];
// Code from a string, which throws on every host a data module runs on through these names: Hermes compiles none, and
// the web's module and render realm refuse it as Hermes does (host/web-js/ts-fetch.js, host/web/storage-environment.js).
// Each use of the global is a warning, never a refusal: a package may catch the throw and take another path (zod's
// `allowsEval`), and what a use reaches through `Function.prototype.constructor` or a function's own `constructor`
// still runs on the web, which no reading of the syntax settles.
const CODE=['Function','eval'];
// TypeScript's nodes that hold code: every other `TS…` node is a type.
const TS_CODE=['TSAsExpression','TSSatisfiesExpression','TSNonNullExpression','TSTypeAssertion','TSInstantiationExpression','TSParameterProperty','TSExportAssignment',
  'TSModuleDeclaration','TSModuleBlock','TSEnumDeclaration','TSEnumBody','TSEnumMember'];
const NODE=['process','Buffer','global','__dirname','__filename','require'];
const SCOPES=['Program','BlockStatement','StaticBlock','SwitchStatement','ForStatement','ForInStatement','ForOfStatement','CatchClause','ClassExpression','TSModuleBlock'];
const NOT_REFERENCES={MemberExpression:'property',Property:'key',MethodDefinition:'key',PropertyDefinition:'key',LabeledStatement:'label',BreakStatement:'label',ContinueStatement:'label',ImportSpecifier:'imported',ExportSpecifier:'exported',ExportAllDeclaration:'exported',MetaProperty:'property',
  TSModuleDeclaration:'id',TSEnumDeclaration:'id',TSEnumMember:'id',TSImportEqualsDeclaration:'id'};
const FUNCTIONS=['FunctionDeclaration','FunctionExpression','ArrowFunctionExpression'];
// The names a scope node declares itself: a function's parameters and its
// hoisted `var`s and functions, a block's lexical declarations, a catch
// clause's parameter, a named function or class expression's own name. Each
// maps to its initializer where a `const` binds it alone, else to null.
//
// Two models. The package refusals' (`full` false) is the one they were decided by,
// and stays so: a change to the runtime model moves no package refusal. The runtime
// model (`full`, used by the warnings and the clock check) includes TypeScript:
// a class's static block and a namespace block are `var` scopes of
// their own (TypeScript emits a namespace as a function); a namespace block also
// sees what every block of the same namespace exports (`model.merged`); a TypeScript
// enum (not a `const` one), a namespace that holds a value and `import X = …`
// bind, and what TypeScript erases binds nothing: a `declare`, an `import type`,
// a `const` enum, a type-only namespace. Where it meets a construct it does not
// model, it binds nothing, so the name is the global. In this model a function's
// body (identified by `parent`) owns its declarations: parameter initializers
// cannot see them. In a `var` scope of sloppy code (`model.sloppy`), a function
// declared in a nested block can also bind its name there (`annexB`).
function declares(node, full=false, model=null, parent=null) {
  const names=new Map();
  const pattern=(p)=>{
    if (!p) return;
    if (p.type==='Identifier') names.set(p.name,null);
    else if (p.type==='ObjectPattern') p.properties.forEach(q=>pattern(q.type==='RestElement' ? q.argument : q.value));
    else if (p.type==='ArrayPattern') p.elements.forEach(pattern);
    else if (p.type==='AssignmentPattern') pattern(p.left);
    else if (p.type==='RestElement') pattern(p.argument);
    else if (full && p.type==='TSParameterProperty') pattern(p.parameter);
  };
  const lexical=(statements)=>{
    for (let s of statements ?? []) {
      if (['ExportNamedDeclaration','ExportDefaultDeclaration'].includes(s.type)) s=s.declaration ?? s;
      if (full) {
        if (s.declare) continue;
        if (s.type==='ImportDeclaration') { if (s.importKind!=='type') s.specifiers.forEach(x=>x.importKind!=='type' && names.set(x.local.name,null)); continue; }
        const own=namespaceName(s);
        if ((s.type==='TSEnumDeclaration' && !s.const) || (own && instantiated(s)) || (s.type==='TSImportEqualsDeclaration' && s.importKind!=='type')) names.set(own ?? s.id.name,null);
      }
      if (s.type==='VariableDeclaration' && s.kind!=='var') s.declarations.forEach(d=>d.id.type==='Identifier' && s.kind==='const' ? names.set(d.id.name,d.init) : pattern(d.id));
      if (['FunctionDeclaration','ClassDeclaration'].includes(s.type) && s.id) names.set(s.id.name,null);
      if (s.type==='ImportDeclaration') s.specifiers.forEach(x=>names.set(x.local.name,null));
    }
  };
  const vars=(n)=>{
    if (!n || typeof n!=='object') return;
    if (Array.isArray(n)) { n.forEach(vars); return; }
    if (FUNCTIONS.includes(n.type) || n.type==='ClassExpression' || n.type==='ClassDeclaration') return;
    if (full && (n.type==='TSModuleDeclaration' || n.type==='StaticBlock')) return;
    if (n.type==='VariableDeclaration' && n.kind==='var' && !(full && n.declare)) n.declarations.forEach(d=>pattern(d.id));
    for (const key in n) if (key!=='parent' && n[key] && typeof n[key]==='object') vars(n[key]);
  };
  if (node.type==='Program') {
    lexical(node.body); vars(node.body);
    if (full && model?.sloppy.has(node)) for (const name of annexB(node.body,[])) names.set(name,null);
  }
  else if (FUNCTIONS.includes(node.type)) {
    node.params.forEach(pattern);
    if (node.type==='FunctionExpression' && node.id) names.set(node.id.name,null);
    if (!full && node.body?.type==='BlockStatement') { lexical(node.body.body); vars(node.body.body); }
  }
  else if (node.type==='ClassExpression' && node.id) names.set(node.id.name,null);
  else if (full && (['StaticBlock','TSModuleBlock'].includes(node.type)
    || (node.type==='BlockStatement' && FUNCTIONS.includes(parent?.type)))) {
    for (const name of model?.merged.get(node) ?? []) names.set(name,null);
    lexical(node.body); vars(node.body);
    if (model?.sloppy.has(node)) for (const name of annexB(node.body,FUNCTIONS.includes(parent?.type) ? parent.params : [])) names.set(name,null);
  }
  else if (['BlockStatement','StaticBlock'].includes(node.type)) lexical(node.body);
  else if (node.type==='SwitchStatement') node.cases.forEach(c=>lexical(c.consequent));
  else if (['ForStatement','ForInStatement','ForOfStatement'].includes(node.type)) { const d=node.init ?? node.left; if (d?.type==='VariableDeclaration') lexical([d]); }
  else if (node.type==='CatchClause') pattern(node.param);
  return names;
}
// The names that sloppy code's functions declared in blocks under `body`, a `var` scope's statements, also bind in
// that scope (ECMAScript Annex B.3.3.1 for a function, B.3.3.2 for a script; B.3.4 for an `if` branch and a catch
// clause): each plain function (not a generator or async one) directly in a block, a case clause or an `if` branch,
// where a `var` of its name in its place would not be an early error. Such an error is a parameter of the name
// (`params`), or a lexical declaration of it in a scope between: a `let`, `const` or class in its own block, or any
// lexical declaration (a block's functions included) in a block, loop head or destructuring catch parameter around
// that.
// A catch clause's lone parameter does not stop it (B.3.4). `body`'s own functions are `var`s, and its lexical
// declarations stop the hoist too. A labelled function in a block is taken as hoisted, as V8 does though the text
// says otherwise: an extra binding only leaves a name unchecked. The rule for eval code (B.3.3.3) does not arise:
// no eval code is checked.
function annexB(body, params) {
  const found=new Set(), stop=declares({type:'ArrowFunctionExpression',params,body:null},true);
  const lexical=(statements,functions)=>declares({type:'BlockStatement',body:(statements ?? []).filter(s=>functions || s.type!=='FunctionDeclaration')});
  const hoist=(f,scopes)=>{
    while (f?.type==='LabeledStatement') f=f.body;
    if (f?.type!=='FunctionDeclaration' || f.generator || f.async || f.declare || !f.id) return;
    if (!scopes.some(s=>s.has(f.id.name))) found.add(f.id.name);
  };
  const walk=(n,scopes)=>{
    if (!n || typeof n!=='object') return;
    if (Array.isArray(n)) { n.forEach(m=>walk(m,scopes)); return; }
    if (FUNCTIONS.includes(n.type) || ['ClassDeclaration','ClassExpression','StaticBlock','TSModuleDeclaration'].includes(n.type)) return;
    let own=null;
    if (['BlockStatement','SwitchStatement'].includes(n.type)) {
      const statements=n.type==='BlockStatement' ? n.body : n.cases.flatMap(c=>c.consequent), here=[...scopes,lexical(statements,false)];
      own=lexical(statements,true);
      statements.forEach(f=>hoist(f,here));
    }
    else if (n.type==='IfStatement') { hoist(n.consequent,scopes); hoist(n.alternate,scopes); }
    else if (['ForStatement','ForInStatement','ForOfStatement'].includes(n.type) || (n.type==='CatchClause' && n.param?.type!=='Identifier')) own=declares(n);
    const inner=own ? [...scopes,own] : scopes;
    for (const key in n) if (key!=='parent') walk(n[key],inner);
  };
  walk(body,[stop,lexical(body,false)]);
  return found;
}
// A namespace's own name, the first of a dotted one (`namespace A.B` binds `A`).
const namespaceName=(s)=>{
  if (s?.type!=='TSModuleDeclaration' || s.declare || s.kind==='global') return null;
  let id=s.id;
  while (id?.type==='TSQualifiedName') id=id.left;
  return id?.type==='Identifier' ? id.name : null;
};
const VALUES=['FunctionDeclaration','VariableDeclaration','ClassDeclaration','ExpressionStatement','TSEnumDeclaration','TSImportEqualsDeclaration'];
function instantiated(ns) {
  const body=ns.body;
  if (!body) return false;
  if (body.type==='TSModuleDeclaration') return instantiated(body);
  return (body.body ?? []).some(statement=>{
    const s=['ExportNamedDeclaration','ExportDefaultDeclaration'].includes(statement.type) ? statement.declaration : statement;
    if (!s || s.declare || (s.type==='TSEnumDeclaration' && s.const)) return false;
    return VALUES.includes(s.type) || (s.type==='TSModuleDeclaration' && instantiated(s));
  });
}
// Each namespace block's names from the blocks it merges with: what every block of its namespace exports, and, for
// `namespace A.B`, what A's blocks export (TypeScript emits each as `A.name`). An enum, class or function merged with
// the namespace adds nothing: its members are not in scope in the namespace's blocks.
function mergedExports(ast) {
  const exported=new Map(), blocks=[];
  const exports=(key,body)=>{
    const set=exported.get(key) ?? new Set();
    exported.set(key,set);
    for (const statement of body ?? []) {
      if (statement.type!=='ExportNamedDeclaration' || !statement.declaration) continue;
      const own=declares({type:'TSModuleBlock',body:[statement]},true);
      for (const name of own.keys()) set.add(name);
    }
  };
  // `prefix`: the enclosing namespace's key; `own`, this block's, for a namespace it does not export, which merges only
  // with those of its own block.
  const visit=(statements,prefix,own)=>{
    for (const statement of statements ?? []) {
      const s=statement.type==='ExportNamedDeclaration' ? statement.declaration : statement;
      if (!namespaceName(s)) continue;
      const parts=[];
      for (let id=s.id; id; id=id.type==='TSQualifiedName' ? id.left : null) parts.unshift(id.type==='TSQualifiedName' ? id.right.name : id.name);
      const base=statement.type==='ExportNamedDeclaration' || !prefix ? prefix : own;
      const keys=parts.map((_,i)=>base+parts.slice(0,i+1).join('.'));
      // `namespace A.B` exports `B` from A.
      keys.slice(1).forEach((key,i)=>{ if (instantiated(s)) (exported.get(keys[i]) ?? exported.set(keys[i],new Set()).get(keys[i])).add(parts[i+1]); });
      let body=s.body;
      while (body?.type==='TSModuleDeclaration') body=body.body;
      if (body?.type!=='TSModuleBlock') continue;
      exports(keys.at(-1),body.body);
      blocks.push([body,keys]);
      visit(body.body,keys.at(-1)+'.',`${keys.at(-1)}#${blocks.length}.`);
    }
  };
  visit(ast.body,'','');
  const merged=new Map();
  for (const [block,keys] of blocks) merged.set(block,keys.flatMap(key=>[...exported.get(key) ?? []]));
  return merged;
}
// What the runtime model reads from the whole module: each namespace block's names from the blocks it merges with
// (`merged`), and the `var` scopes of sloppy code (`sloppy`). Code is strict in an ES module, under a "use strict"
// directive and in a class. The parser reads a file without `import` or `export` as a script, which a CommonJS
// module is, and the bake's IIFE output keeps it sloppy; a script the bundler takes as an ES module is strict there,
// and taking it as sloppy only leaves a name a block function shadows unchecked.
function runtimeModel(ast) {
  const sloppy=new Set();
  const directive=(body)=>{
    for (const s of body ?? []) {
      if (s.type!=='ExpressionStatement' || typeof s.directive!=='string') return false;
      if (s.directive==='use strict') return true;
    }
    return false;
  };
  const walk=(node,strict)=>{
    if (!node || typeof node!=='object') return;
    if (Array.isArray(node)) { node.forEach(n=>walk(n,strict)); return; }
    if (node.type==='Program') strict=node.sourceType==='module' || directive(node.body);
    else if (['ClassDeclaration','ClassExpression'].includes(node.type)) strict=true;
    else if (FUNCTIONS.includes(node.type) && node.body?.type==='BlockStatement') {
      strict ||= directive(node.body.body);
      if (!strict) sloppy.add(node.body);
    }
    if (!strict && ['Program','TSModuleBlock'].includes(node.type)) sloppy.add(node);
    for (const key in node) if (key!=='parent') walk(node[key],strict);
  };
  walk(ast,false);
  return {merged:mergedExports(ast),sloppy};
}
// The names `node` binds at run time for the code under it (`declares`' `full` model), or null where it opens no
// scope. A switch's case bindings are in scope in its cases, not its discriminant; an enum's members are in scope in
// its initializers.
function runtimeScope(node, parent, model) {
  if (node.type==='SwitchStatement') return null;
  if (node.type==='SwitchCase') return declares(parent,true,model);
  if (node.type==='TSEnumBody') return new Map(node.members.map(m=>[m.id.name ?? m.id.value,null]));
  return SCOPES.includes(node.type) || FUNCTIONS.includes(node.type) ? declares(node,true,model,parent) : null;
}
/** Visit executable syntax with what binds each name at run time (`runtimeScope`): `local.has(name)` is whether
 * an enclosing scope binds `name` there; `local.scope(name)` identifies that scope. The visitor also receives
 * the node's parent. */
export function walkRuntime(ast, visit) {
  const scopes=[], model=runtimeModel(ast), local={has:name=>scopes.some(s=>s.has(name)), scope:name=>scopes.findLast(s=>s.has(name))};
  const walk=(node,parent)=>{
    if (!node || typeof node!=='object') return;
    if (Array.isArray(node)) { node.forEach(n=>walk(n,parent)); return; }
    if (node.declare || (node.type?.startsWith('TS') && !TS_CODE.includes(node.type))) return;
    const names=runtimeScope(node,parent,model);
    if (names) scopes.push(names);
    visit(node,local,parent);
    for (const k in node) if (k!=='parent') walk(node[k],node);
    if (names) scopes.pop();
  };
  walk(ast,null);
}
/** The DOM and Node globals a JavaScript file uses, as `label(at)`'s
 * `…: why`: each use of a name no enclosing scope binds, not guarded by a
 * `typeof` test of it that dominates the use (`typeof x !== 'undefined' &&
 * x.y`, its `if` or `?:`, or an earlier `if (typeof x …) return`; where the
 * test is true only with `x` there, also `typeof x?.y === 'object'` or a
 * `const` initialized with such a test), and each such member of
 * `globalThis`, `self`, `window` or `global`. A member of the global object
 * reads `undefined` where the name is absent and throws nothing, so such a
 * read, or an optional chain from it (`globalThis.process?.env`), is not
 * refused where that value is only tested, compared, spread into an object,
 * returned, or the whole initializer of a `const`, whose own uses are then
 * held to the same rules. Static: a use behind any other alias still reaches
 * an executor without the name. Each use of the global `Function` or `eval`
 * (where no scope binds the name at run time, `declares`' `full` model) goes to
 * `warnings` instead, as `where(at) warning [code-from-string] text` (`CODE`).
 * `lang`: the parser's, `ts` or `tsx` for TypeScript, whose refusals the caller
 * does not take. */
export function packageRefusals(code, label, parse, warnings=[], lang='js', where=label) {
  const ast=parse(code,{lang}), found=[], at=lineColumn(code);
  // `path`: each ancestor as [node, key, index, how many scopes enclose its child].
  // `runtime`: the scopes of the warnings' model (`runtimeScope`).
  const scopes=[], runtime=[], path=[], model=runtimeModel(ast);
  const free=(name)=>!scopes.some(s=>s.has(name)), global=(name)=>!runtime.some(s=>s.has(name));
  const binding=(name,depth=scopes.length)=>scopes.slice(0,depth).findLast(s=>s.has(name))?.get(name);
  // Whether `node` tests `typeof name` (or `typeof globalThis.name`).
  const tests=(node,name)=>{
    if (!node || typeof node!=='object') return false;
    if (Array.isArray(node)) return node.some(n=>tests(n,name));
    if (node.type==='UnaryExpression' && node.operator==='typeof') {
      const a=bare(node.argument);
      return a?.type==='Identifier' ? a.name===name : propertyName(a)===name;
    }
    if (FUNCTIONS.includes(node.type)) return false;
    for (const key in node) if (key!=='parent' && tests(node[key],name)) return true;
    return false;
  };
  // Whether `test` is true only where `name` is: `typeof name !== 'undefined'` or `=== 'object'`, the same of
  // `name?.y` or `globalThis.name`, an `&&` holding one, or a `const` initialized with one (`depth` scopes in).
  const shows=(test,name,depth,hops=0)=>{
    test=bare(test);
    if (test?.type==='LogicalExpression' && test.operator==='&&') return shows(test.left,name,depth,hops) || shows(test.right,name,depth,hops);
    if (test?.type==='Identifier' && hops<4) { const init=binding(test.name,depth)?.init; return !!init && shows(init,name,depth,hops+1); }
    if (test?.type!=='BinaryExpression' || !['===','!==','==','!='].includes(test.operator)) return false;
    const [probe,kind]=test.left?.type==='UnaryExpression' ? [test.left,test.right] : [test.right,test.left];
    if (probe?.type!=='UnaryExpression' || probe.operator!=='typeof' || kind?.type!=='Literal' || typeof kind.value!=='string') return false;
    if ((kind.value==='undefined')===test.operator.startsWith('=')) return false;
    const a=bare(probe.argument), object=a?.type==='MemberExpression' ? bare(a.object) : null;
    return a?.type==='Identifier' ? a.name===name
      : object?.type==='Identifier' && (object.name===name ? a.optional : HOSTS.includes(object.name) && propertyName(a)===name);
  };
  const exits=(s)=>s && (['ReturnStatement','ThrowStatement'].includes(s.type) || (s.type==='BlockStatement' && exits(s.body.at(-1))));
  // Whether a use of `name`, under the ancestors `path` (outermost first), is guarded.
  const guarded=(name,path)=>path.some(([node,key,index,depth])=>{
    if (['IfStatement','ConditionalExpression'].includes(node.type) && key!=='test' && tests(node.test,name)) return true;
    if (['IfStatement','ConditionalExpression'].includes(node.type) && key==='consequent' && shows(node.test,name,depth)) return true;
    if (node.type==='LogicalExpression' && key==='right' && (tests(node.left,name) || (node.operator==='&&' && shows(node.left,name,depth)))) return true;
    // An earlier statement of an enclosing block leaves when the name is absent.
    const list=key==='body' || key==='consequent' ? node[key] : null;
    return Array.isArray(list) && list.slice(0,index).some(s=>s.type==='IfStatement' && tests(s.test,name) && exits(s.consequent));
  });
  // The refused name a global-object member names (`globalThis.process`), else null.
  const member=(node)=>{
    const object=bare(node?.object), name=propertyName(node);
    return node?.type==='MemberExpression' && object?.type==='Identifier' && HOSTS.includes(object.name) && free(object.name)
      && (DOM.includes(name) || NODE.includes(name) || HOSTS.includes(name)) ? name : null;
  };
  // The refused name a `const`'s initializer reads: such a member, another `const` that holds one, or an optional
  // chain from either (`globalThis.process?.env`, `node?.env`). Resolved where the `const` is declared.
  const rooted=(init)=>{
    let node=bare(init), link=null;
    for (;;) {
      const name=node?.type==='Identifier' ? binding(node.name)?.held : member(node);
      if (name) return !link || link.optional ? name : null;
      if (node?.type==='MemberExpression') [link,node]=[node,node.object];
      else if (node?.type==='CallExpression') [link,node]=[node,node.callee];
      else return null;
    }
  };
  // Whether the value that `path[i]` holds is read where `undefined` throws nothing and goes nowhere the check cannot
  // follow: tested, compared, spread into an object, returned, or the whole initializer of a `const` (which `rooted`
  // then follows). Through `||`, `??` or `?:` it is no longer the whole initializer.
  const flows=(i,whole=true)=>{
    const [p,k]=path[i] ?? [];
    switch (p?.type) {
      case 'ParenthesizedExpression': return flows(i-1,whole);
      case 'ExpressionStatement': case 'ReturnStatement': return true;
      case 'IfStatement': case 'WhileStatement': case 'DoWhileStatement': case 'ForStatement': return k==='test';
      case 'ConditionalExpression': return k==='test' || flows(i-1,false);
      case 'UnaryExpression': return ['!','typeof','void'].includes(p.operator);
      case 'BinaryExpression': return ['===','!==','==','!='].includes(p.operator);
      case 'LogicalExpression': return (k==='left' && p.operator!=='&&') || flows(i-1,false);
      case 'SpreadElement': return path[i-1]?.[0]?.type==='ObjectExpression';
      case 'ArrowFunctionExpression': return k==='body';
      case 'VariableDeclarator': return whole && k==='init' && p.id.type==='Identifier' && path[i-1]?.[0]?.kind==='const';
      default: return false;
    }
  };
  // Whether the value at the walk's node, `undefined` where a global is absent, is used safely: as `flows` says,
  // or as the head of an optional chain (`x?.y.z`) whose whole value is.
  const safe=()=>{
    let i=path.length-1;
    const link=([p,k]=[])=>(p?.type==='MemberExpression' && k==='object') || (p?.type==='CallExpression' && k==='callee');
    if (!link(path[i])) return flows(i);
    if (!path[i][0].optional) return false;
    while (i>0 && link(path[i-1])) i--;
    return path[i-1]?.[0]?.type==='ChainExpression' && flows(i-2);
  };
  // Not warned: `Function.prototype`'s named members other than `constructor` (`Function.prototype.bind`), and either side
  // of `instanceof`, which build no code from a string.
  const inert=(parent,key,grand)=>(parent?.type==='BinaryExpression' && parent.operator==='instanceof')
    || (['MemberExpression','JSXMemberExpression'].includes(parent?.type) && key==='object' && propertyName(parent)==='prototype'
      && ['MemberExpression','JSXMemberExpression'].includes(grand?.type) && grand.object===parent && ![null,'constructor'].includes(propertyName(grand)));
  // Whether the name at `parent`'s `key` is declared there (a binding, erased or not), not used.
  const DECLARED={VariableDeclarator:'id',FunctionDeclaration:'id',FunctionExpression:'id',ClassDeclaration:'id',ClassExpression:'id',TSDeclareFunction:'id',
    ImportSpecifier:'local',ImportDefaultSpecifier:'local',ImportNamespaceSpecifier:'local',TSImportEqualsDeclaration:'id',CatchClause:'param',RestElement:'argument',AssignmentPattern:'left'};
  const declaring=(parent,key,grand)=>DECLARED[parent?.type]===key || key==='params' || parent?.type==='ArrayPattern'
    || (parent?.type==='Property' && key==='value' && grand?.type==='ObjectPattern');
  // A use of the global `Function` or `eval`, once per site, as a diagnostic in the compiler's form
  // (`file:line:col warning [id] text`, scripts/app-cli.mjs `compilerWarnings`).
  const fromString=(name,node)=>warnings.push(`${where(':'+at(node.start))} warning [code-from-string] uses '${name}', which builds code from a string: through this name that throws on every host, so the code fails there unless it catches the throw and takes another path, as zod does; what it reaches through \`Function.prototype.constructor\` or a function's own \`constructor\` still runs on the web, not on Hermes`);
  // Taking the property is the reference; the pattern's binding and later alias uses add no site.
  const destructured=(pattern,value)=>{
    const object=bare(value);
    if (pattern?.type!=='ObjectPattern' || object?.type!=='Identifier' || !HOSTS.includes(object.name) || !global(object.name)) return;
    for (const property of pattern.properties) {
      const name=propertyName(property);
      if (CODE.includes(name)) fromString(name,property.key);
    }
  };
  const refuse=(name,node)=>{
    const where=label(':'+at(node.start));
    if (name==='import()') found.push(where+` loads code from a string ('${name}'), which a bundle cannot capture`);
    else if (HOSTS.includes(name) && !DOM.includes(name)) found.push(where+` uses the page's '${name}' through the global object: a data module has no page`);
    else if (DOM.includes(name)) found.push(where+` uses the DOM's '${name}': a data module has no DOM`);
    else if (name==='require') found.push(where+' loads a module by a computed name, which a bundle cannot capture');
    else found.push(where+` uses Node's '${name}': a data module runs on Hermes and in the browser, which have no Node APIs`);
  };
  const walk=(node,parent,key)=>{
    if (!node || typeof node!=='object') return;
    // TypeScript's types name no value: only its expression wrappers (`x as T`) hold code, and their types are skipped.
    if (typeof node.type==='string' && node.type.startsWith('TS') && (!TS_CODE.includes(node.type) || node.declare)) return;
    if (node.type==='JSXClosingElement') return;
    if (Array.isArray(node)) { node.forEach((n,i)=>{ path.push([parent,key,i,scopes.length]); walk(n,parent,key); path.pop(); }); return; }
    // `typeof x` and `typeof globalThis.x` answer without `x`; `typeof x.y` reads `x`, as any use does.
    const probed=bare(node.argument), host=bare(probed?.object);
    if (node.type==='UnaryExpression' && node.operator==='typeof' && (probed?.type==='Identifier'
      || (probed?.type==='MemberExpression' && host?.type==='Identifier' && HOSTS.includes(host.name)
        && (free(host.name) || global(host.name)) && propertyName(probed)!==null))) return;
    const pushed=SCOPES.includes(node.type) || FUNCTIONS.includes(node.type);
    if (pushed) {
      const names=declares(node);
      scopes.push(names);
      for (const [name,init] of names) if (init) names.set(name,{init,held:rooted(init)});
    }
    const own=runtimeScope(node,parent,model);
    if (own) runtime.push(own);
    const grand=path.at(-2)?.[0];
    // `export { x } from './x.js'` names another module's export.
    const reference=!(NOT_REFERENCES[parent?.type]===key && !parent.computed) && !(parent?.type==='ExportSpecifier' && grand?.source);
    // The global `Function` or `eval`, however reached as a name, but where the name is declared (an erased binding,
    // which the scopes leave out) and in a use that builds nothing (`inert`). Lowercase JSX names are tags unless
    // they head a member expression (`eval.Component`); a closing name repeats the opening reference.
    const jsx=node.type==='JSXIdentifier' && ((parent?.type==='JSXOpeningElement' && key==='name' && /^[A-Z]/.test(node.name))
      || (parent?.type==='JSXMemberExpression' && key==='object'));
    if (((node.type==='Identifier' && reference && !declaring(parent,key,grand)) || jsx)
      && CODE.includes(node.name) && global(node.name) && !inert(parent,key,grand)) fromString(node.name,node);
    if (node.type==='VariableDeclarator') destructured(node.id,node.init);
    if (['AssignmentExpression','AssignmentPattern'].includes(node.type)) destructured(node.left,node.right);
    if (node.type==='Identifier' && reference && free(node.name)) {
      const name=node.name;
      const constant=name==='process' && parent?.type==='MemberExpression' && key==='object' && propertyName(parent)==='env' && grand?.type==='MemberExpression' && propertyName(grand)==='NODE_ENV';
      const literal=name==='require' && parent?.type==='CallExpression' && key==='callee'
        && ['Literal','TemplateLiteral'].includes(parent.arguments[0]?.type) && !parent.arguments[0].expressions?.length;
      if ((DOM.includes(name) || NODE.includes(name)) && !constant && !literal && !guarded(name,path)) refuse(name,node);
    }
    else if (node.type==='Identifier' && reference && !(parent?.type==='VariableDeclarator' && key==='id')) {
      const name=binding(node.name)?.held;
      if (name && !guarded(node.name,path) && !safe()) refuse(name,node);
    }
    if (['MemberExpression','JSXMemberExpression'].includes(node.type)) {
      const object=bare(node.object), name=propertyName(node);
      // Not under a `typeof` test, as an absent global is: the stand-in is a function, and the call throws.
      if (CODE.includes(name) && ['Identifier','JSXIdentifier'].includes(object?.type) && HOSTS.includes(object.name) && global(object.name)
        && !inert(parent,key,grand)) fromString(name,node);
      if (member(node) && !guarded(name,path) && !safe()) refuse(name,node);
    }
    // `import(x)` of a computed name, like a computed `require`.
    if (node.type==='ImportExpression' && !(['Literal','TemplateLiteral'].includes(node.source?.type) && !node.source.expressions?.length)) refuse('import()',node);
    for (const k in node) if (k!=='parent' && node[k] && typeof node[k]==='object') {
      if (!Array.isArray(node[k])) path.push([node,k,undefined,scopes.length]);
      walk(node[k],node,k);
      if (!Array.isArray(node[k])) path.pop();
    }
    if (pushed) scopes.pop();
    if (own) runtime.pop();
  };
  walk(ast,null,null);
  return found;
}
