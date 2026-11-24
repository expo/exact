# Vendored Ibex split crates — Exact integration ledger

## Android 10 and Intl

Android selects Intl from the validated bundle receipt. The pinned v7
bundles enable it. A receipt enabling Hermes Intl must authenticate the
base, English and full ICU data tiers, as on Linux. `hermes-lean-sys` exports
that availability to both the Ibex installer and Exact's Rust/C++ script lists.
Only the selected data archive links; Exact selects English. The link directives
keep the receipt’s VM, ICU, then platform-runtime order. Ibex supplies
NumberFormat and DateTimeFormat; Hermes's ICU backend supplies Collator.
Stub builds emit empty Intl bytecode beside the other binding placeholders.

The pinned Android bundle targets API 29. The resolver applies Android's ICU
layout and receipt requirements to arm64 and x86_64. Local development bundles
can be selected through `HERMES_LEAN_SYS_DIR_aarch64_linux_android` and
`HERMES_LEAN_SYS_DIR_x86_64_linux_android`; the pinned v7 release supplies both.

The installer’s `--check-dir <directory> <rust-triple>` validates a local cross
bundle with the build resolver and cached host compiler, without installing or
downloading it. `scripts/hermes-android.mjs` checks each extracted candidate
before publishing it or changing `current`. Cross-bundle admission verifies every
receipt-listed archive, header, compiler and ICU filter against its digest,
independently of the selected Intl data tier. Android’s JSI, Boost and base ICU archives
must be present in the manifest.

## Android certificate verification

HTTP and WebSocket TLS share `transport/tls.rs`. Android uses
`rustls-platform-verifier`, initialized with the application's JVM context before
network work starts. The local Kotlin companion in `crates/ibex2/android` uses
`AndroidCAStore` for the phone's system and user-installed roots unless the base
APK's manifest declares a network security configuration; it makes that trust
manager again after `KeyChain.ACTION_TRUST_STORE_CHANGED`, because a manager
over an explicit keystore copies its anchors once. With that declaration,
it initializes the default trust manager with a null keystore to honor the app's
anchors and exclusions. Both paths pass the hostname to
`X509TrustManagerExtensions`. The companion implements the 0.7 JNI interface,
unchanged in 0.7.1 (Android artifact 0.2.0); `CertificateVerifierTest` checks its
static method, result fields and status codes. The Canvas reader compiles these
classes and keeps them through R8. Other Android consumers must compile these
classes instead of the published Android AAR and call
`transport::initialize_android` before TLS; an uninitialized consumer receives
an error.

## Distrusted emSign keys

Off Android, the portable transports trust what `webpki-roots` compiles in when
the system has no roots, and verify with `rustls-webpki`. Both locks, like Exact's root
lock, hold `webpki-roots` 1.0.12, without the emSign roots Mozilla removed
(RUSTSEC-2026-0381), and `rustls-webpki` 0.103.17, which refuses a path holding
the distrusted emSign intermediate keys (RUSTSEC-2026-0382). Exact's
`transport/trust_tests.rs`, with its certificates in
`transport/testdata/distrusted/`, checks both against either kind of store;
replay it on a refresh unless upstream carries the same check.

## Repeated portable HTTP response fields

`transport/rustls_http.rs` combines repeated `Cache-Control` and `Vary` fields
in wire order, preserving every restriction and selector. Other fields keep
their last value, including `Content-Type` for stream classification,
`Location` for redirects and `Set-Cookie`. Exact's
`host/executor/src/stream_tests.rs` checks event delivery before connection close
(off Apple, where URLSession joins repeated fields).
`host/executor/src/shared_cache_tests.rs` exercises repeated
wire fields through the shared cache; `host/render/tests/it/http_cache.rs`
checks that separate request renders still fetch private responses.

## Windows WebSocket readiness

`crates/ibex2/src/transport/websocket.rs` subscribes to writable readiness only
while output is pending. Windows rearms readiness after a blocked read; keeping
writable interest on an empty output queue makes an outstanding receive spin.
The idle-receive regression in `websocket_tests.rs` checks that the pump stays
parked and still wakes on cancellation.

## Mac widget Keychain (LLP 1126.001 WP16)

`crates/ibex2/src/secrets/darwin_keychain.mm` selects the restricted
`ExactKeychainAccessGroup` in the data-protection Keychain for a provisioned
Apple widget cohort. Ordinary Mac apps keep the login Keychain. The containing
app migrates its service's login items on its first provisioned launch;
copy failure keeps the old item, and an existing shared item wins. Migration
runs once per process, from `prepare` only. An unreadable item is skipped;
errors are logged without withholding the app's I/O. Retained items can be
retried on the next launch. The move is one-way. Extensions never migrate.
The Objective-C fixture in
`host/apple/tests/fixtures/keychain-migration.mm` exercises retries and failures
with substituted Security calls; signed Keychain access still needs profiles.

## Snapshot and update recipe

- Upstream: `https://github.com/expo/ibex.git` at
  `a0183568e8229b8243629cdb26cd4a88ac4e2abd` (2026-10-30).
- Source trees: `crates/{ibex2,ibex2-sqlite,hermes-lean-sys,hermes-lean-sys-installer}`.
- Data inputs: `scripts/icu74-filter-{root-en,en-intl}.json` and
  `third_party/wpt/urltestdata.json`.
- Hermes bundle is `hermes-vanilla-d412d3bd8512-v7`, source
  `d412d3bd…` / Hermes `260318099.0.4`. Repository engine-output directories
  remain absent; builds validate the paired compiler and VM in the bundle cache.
- The two standalone `Cargo.lock` files under `ibex2` and
  `hermes-lean-sys-installer` are consumer integration files. The installer
  shares pinned dependency versions with Exact's root lock.

The October 13 refresh compared the old `7f77c82a12f20a4cd3ebbe01fdaa7f22deb7c985`
upstream tree, Exact `3ad9ef9cb396fc1cc786abcec03cc1b6bc21cec5`'s vendor tree,
and the new upstream tree before replacing any source. The old local delta
included 40 changed/added files. Reusable changes now come from upstream;
Exact's explicit stub configuration and standalone locks are replayed below.

The follow-up refresh from `b20dded` to `bf74e54` changed only
`fs_image.rs` rustdoc within the vendored inputs: opaque native codec workspace
is a measured admission estimate, not an allocator ceiling. Both stub seams,
the three standalone workspace boundaries, both locks, and Exact's app-facing
storage declaration were unchanged. The subsequent `e60b3e0` refresh adopts
the authenticated v5 trust table: eleven target mappings for ten assets,
including Android aarch64. Its manifests, locks and integration seams are
unchanged. The published bundle was consumed by the qualification below.
The `e742c72` refresh adopts expo/ibex#1: off Apple, the portable WebSocket
transport tunnels through the environment's proxy as fetch does
(`transport/websocket.rs` and its tests, the only vendored inputs that changed
since `e60b3e0`). Manifests, locks and integration seams are unchanged.

The October 18 refresh compared that `e742c72` upstream tree, Exact
`a28fc4f18fda0da187f0a8e564f8407c57748dd8`'s vendor tree, and the new
`bbf47f3` upstream tree before replacement. Upstream now supplies the socket
queue count, scheme-wide grants, allocation-reduced descriptor reads and the
collection between the hardening guard and `harden.js`; their active local
patches are retired below. The refresh also adopts fail-closed portable fetch
for an unsupported environment SOCKS proxy, including a `NO_PROXY` bypass and
one proxy snapshot per transport. The eight changed upstream inputs are
`bindings/install.cc`, `grant.rs`, `stdlib/websocket.rs`, the two Darwin
WebSocket files, `rustls_http.rs`, `transport/websocket.rs`, and its tests.
The v5 engine trust table, manifests, data inputs and standalone locks did not
change. After replacement, the complete Exact delta apart from this ledger is
ten files: the two locks, three workspace boundaries, two stub build seams,
the app-facing storage declaration, Keychain group selection and read-only
directory snapshots.

The October 30 refresh compared `bbf47f3`, Exact `d16c84ee03`'s vendor tree, and
the new `a018356` upstream tree (the merges of expo/ibex#3 and #4). Upstream now
supplies `AbortSignal.any`'s Web IDL sequence conversion and the abort hooks'
view of a composed signal's `sources`, so local patches 25 and 22 are retired;
the portable transport's resend of a request whose kept connection closes
unanswered (`transport/rustls_http.rs`, which exact2's Linux OAuth requests
met); and Fetch's `Headers` guard, which a fetched response's headers take and
an embedder may give its own through `InstallOptions::headers_hooks`. The nine
changed upstream inputs are `include/ibex2_jsi.h`, `bindings/abort.js`,
`bindings/fetch.js`, `bindings/headers.js`, `bindings/install.cc`,
`secrets/mod.rs`, `transport/darwin_websocket.rs`, `transport/rustls_http.rs`
and `transport/websocket_tests.rs`; five were also Exact's. `install.cc`,
`ibex2_jsi.h` and the websocket tests merged without overlap, and `abort.js` is
upstream's. `rustls_http.rs` sends patch 23's counted reader through the resend
(`send`), the reader rewound so its bytes sent are told again from zero.
Manifests, locks, data inputs and the bundle pin did not change.

For another refresh, export the four named crate trees and all three data inputs
from the selected commit into a temporary directory. Recover the current local
delta against the pin above before replacing them; classify each difference
against both snapshots. Replay the integration seams, regenerate the standalone
locks, verify installer/root overlap, and run both upstream and Exact consumers.
Do not copy local engine outputs or silently replace the bundle pin.

```sh
cargo generate-lockfile --manifest-path vendor/ibex/crates/hermes-lean-sys-installer/Cargo.toml --offline
cargo metadata --manifest-path vendor/ibex/crates/hermes-lean-sys-installer/Cargo.toml --locked --offline --format-version 1 --no-deps
```

After regeneration, align every installer's dependency also present in Exact's
root lock with that root version using `cargo update -p NAME --precise VERSION`.
The lock overlap check is:

```sh
bun -e 'const fs=require("node:fs"),read=p=>Bun.TOML.parse(fs.readFileSync(p,"utf8")).package,root=new Map; for(const p of read("Cargo.lock")){const v=root.get(p.name)||[];v.push(p.version);root.set(p.name,v)} const bad=read("vendor/ibex/crates/hermes-lean-sys-installer/Cargo.lock").filter(p=>root.has(p.name)&&!root.get(p.name).includes(p.version));if(bad.length)throw new Error(`installer/root lock mismatch: ${bad.map(p=>`${p.name}@${p.version}`).join(", ")}`)'
```

## Reusable patch adoption

Numbers preserve the former ledger so a patch never disappears without its
replacement and consumer evidence. Rows 3–7 and 9–13 retire the corresponding
October 13 source patches. The four named rows added on October 18 retire the
last reusable source patches. Platform-specific limits are named rather than
inferred from source replacement. The remaining consumer deltas are the seams
listed below.

| Former Exact patch | Upstream replacement | Exact consumer evidence |
|---|---|---|
| 3. Rustls on macOS | Explicit `portable-http` feature; native Apple default retained | Render tests pass with the feature; native HTTP deadline/drain tests pass in default and async modes |
| 4. Shared `exact-grants`, including `doc:/` | Ibex owns an independent grant grammar; no Exact dependency | Shared fixture passes both Rust grammars and the web matcher (111 browser assertions) |
| 5. Chosen documents | Typed provider and owned `DocumentLease`, with retained directory/file capability and scope guard | Native storage (22 tests), Apple document/session tests and real lean JS tests pass, including replacement and result caps |
| 6. Denied redirect origin | `HostError::DeniedRedirect`, shared hop admission | Runner redirect/refusal cases and actual slow-redirect total-deadline test pass |
| 7. Per-fetch timeout | Request timeout plus asynchronous exchange lifecycle | Actual redirect chain and silent/trickling server cancellation pass in default and async modes |
| 9. Image compression | Admitted `ImageJob`, bounded `ImageService`, owner-scoped work handles and atomic commit arbitration | Native codec, winning/losing cancellation with actual output consumption, and real lean JS (108 tests) pass; private ImageIO workspace qualification below |
| 10. Capped app reads/open handle | Public `AppDirectories::open_file` and `read_bounded` on Unix/Windows | Opened-handle upload, native single-copy and portable double-copy admission/accounting cases pass |
| 11. Host work in flight | Context count plus owned registry/worker/completion accounting | Owned-handle retirement and cancellation witnesses pass; JS continuation semantics below |
| 12. Bounded native filesystem results | Explicit `ResultLimits`; bounded reads/enumeration and batched recursive removal | Exact 16 MiB native/app/document cap tests pass, preserving the typed result-limit error |
| 13. Async fetch | Fallible preparation, infallible accepted activation, terminal retirement, bounded portable workers | Admission refusal, physical retirement, all six base-start failure positions and bounded-worker lifetime tests pass |
| Windows chosen-document EISDIR | Same retained-handle native document executor | Source patch retired against upstream native Windows tests; native Windows Exact storage, document and ordinary TypeScript host requalification is recorded below |
| Socket queued-message count | `MessageSender::buffered_messages`, implemented by the portable sender and the Darwin sender under its lock; the Darwin source test observes a paused native send complete | Exact's native executor checks a batch whole before the transport's 256-message limit; `executor_socket_tests::a_batch_is_refused_whole_past_the_buffer_or_after_a_close` is the direct consumer |
| Scheme-wide network grants | Ibex `GrantSet` owns `http:`/`https:` fetch grants and `ws:`/`wss:` socket grants, with independent family admission tests | `exact-grants` compares Ibex and Exact against `host/web/tests/fixtures/grants.json`; the web matcher consumes that same corpus |
| Presence policy on `secret.keep` | Ibex `GrantSet` reads `secret.keep <name> <policy> [optional] [platform words]` (exact2 LLP 1133.010 D4) and refuses `exact.presence.` names; the grant stays the name's | `exact-grants` compares Ibex and Exact against `host/web/tests/fixtures/grants.json`'s presence cases |
| Allocation-reduced descriptor reads | Ibex captures `hasOwnProperty` and reuses property identifiers while still walking captured own-property descriptors | The linked-lean `exact-js` bootstrap tests refuse Proxy-hidden abort hooks and prove every descriptor-reachable payload is frozen |
| Collection after the hardening guard | Ibex collects only after a trusted bootstrap guard ran, before evaluating `harden.js` | The same linked-lean bootstrap tests exercise the guarded and unguarded paths with production bindings |

Upstream `f28b08d`'s portable-fetch tests run the unsupported SOCKS cases in
child processes, prove refusal before DNS or a direct connection, prove
`NO_PROXY` bypass, and prove a pooled transport retains one supported proxy
snapshot. Exact's render and non-Apple executors consume that portable path.

Patch 9's app interface stays Exact's and is replayed on a refresh:
`bindings/storage.d.ts` declares `compressImage(from, to, {maxDimension,
maxBytes, quality, type})` resolving to `{path, type, size, width, height}`, as Exact's docs
give it, and Exact's prelude (`js/src/prelude.js`) adapts that to upstream's
positional binding (`compressImage(from, to, maxDimension, maxBytes)`, replying
`width\theight\tsize`). Exact adds a fifth positional argument, the media type
(exact2 LLP 1069.002 A1.11, 2026-10-20): `boundary_abi.rs` reads it into
`fs_image::ImageRequest::format` (`ImageFormat::Jpeg` when absent, `Png`; any
other value is an invalid argument), and the codec is Exact's. The fourth,
`maxBytes`, may be undefined or null (exact2 LLP 1069.002 A1.12, 2026-11-14):
`ImageRequest::max_bytes` is an `Option`, `None` for no byte target, and the
codec's output is then bounded by the service's `max_output_bytes`
(`ImageJob::prepare`, `execute`); the call's `quality` rides in patch 9a's
options JSON, which the codec reads.
`fs_image.rs`'s `a_compression_without_a_byte_target_is_bounded_by_the_service`
pins it. Replay all three on a refresh. The October 13 refresh replaced the declaration with
the positional form; `contract/cli/tests/it/typescript.rs` now pins it. Upstream
now preserves Exact's app error branches: a source over the read cap is
`compressImage: too-large:`, and a trial deadline before publication is
`compressImage: timeout:`. This retires the local substring-based image error
patch; exact internal markers and typed deadline errors are matched upstream.
The source pin also deallocates an upload's bytes before invoking its last-release
callback, so the executor cannot refund memory while the allocation is retained.

Patch 9a (exact2 LLP 1133.019 D14, D15, D17, 2026-10-24) is Exact's alone and is
replayed on a refresh. `bindings/install.cc` names a second fs method,
`transformImage`, on op 121. Its native arguments are `(from, to, optionsJson)`;
`compressImage` may add a sixth, its edit's JSON. `boundary_abi.rs` admits both
shapes and passes the JSON unread in `fs_image::ImageRequest::options`, with
`transform` set for the three-argument form. `ImageRequest` is `Clone`, not
`Copy`, and has a `Default`. A transform's `max_dimension` and `max_bytes` are
the service's own (`ImageJob::prepare`), and its failures are named
`transformImage:` (`failed_as`, `WorkControl::check`). The codec's
`CompressedImage` gains `report` (a JSON object, passed through) and `extension`.
A transform whose `to` leaf has no `.` is written at `to.<extension>`, after
`fs.write` is admitted for that path, in the parent already pinned
(`AtomicTarget::extended`). `ImageFile` gains `path` and `report`, and a
transform replies `width\theight\tsize\tpath\treport`. `bindings/storage.d.ts`
declares `transformImage`, `TransformImageOptions`, `TransformedImage`,
`ImageCrop`, and `rotate`, `flip` and `crop` on `CompressImageOptions`.
`fs_image.rs`'s `a_transform_passes_its_options_and_extends_an_extensionless_target`
and `contract/cli/tests/it/typescript.rs` pin it.
The standalone manifests, dependency graph and locks are unchanged by this refresh.

### Historical v4 Windows consumer qualification — 2026-10-13

The Windows lane started from Exact `dfa6a26344cf238f4b642805492806795e01ddfd`
and qualified its corrected codec-refusal assertions and native host at
`b76971320c5c43c505b29b0e3195bf66020c27be`, with Ibex source pin
`de11540`. That lane is integrated in Exact `4fd19fb4e2`. It ran on x86_64
Windows with the authenticated
`hermes-vanilla-d412d3bd8512-v4` bundle. Its receipt uses schema
`ibex/hermes-upstream-pinned-receipt/2` and has SHA-256
`38714ce6a52d8511b4a0f03128ec1a3f4b6338d9e0d6572afbb2fd4d29819bb0`.

`cargo test --locked --offline -j 4 -p exact-data-host -p exact-js
--no-fail-fast` passed 25 data-host tests, 2 JS library tests and 101 JS
integration tests; one manual startup diagnostic stayed ignored. This includes
native chosen-document reads and writes, queued document ownership, storage
result bounds, redirect/deadline/cancellation contracts, and the linked-lean
test that loads its bytecode prelude and refuses source text. A host without an
installed image codec refuses after grant admission and before source/provider
reads; Apple retains its ImageIO assertions.

The existing ignored ordinary TypeScript fixture then built through
`bun host/windows/build.mjs` with four Cargo jobs. The native agent launched
the packaged EXE at 640×360 and delivered two platform button presses: its
Hermes-backed resource changed from `ts count: 0` to `1` and `2`, and the
owned process exited. The EXE SHA-256 is
`e40dadb300adcf003e2e28f16951b6bc7f9372480db81e7fa39967d3ebc08516`.

This does not qualify an actual HTTP exchange through `exact-render` on
Windows. Its library still imports Unix-only `UnixStream`, `AsRawFd` and
`libc::poll`, so the focused loopback executor test does not compile there.
`issues/20261013-exact-render-windows-http.md` records that host boundary.

The already upstream pre-split Windows connect readiness, Windows filesystem
backend, and lazy Rustls trust-store changes remain upstream.

## 8. Android bundle pin — adopted and cross-link qualified

Upstream source now supplies the per-target `HERMES_LEAN_SYS_DIR_<target>`
override, Android link recipe and release target. The consumer now pins the
published v5 bundle, including `aarch64-linux-android`. The release's complete
checksum inventory, archive attestations and paired compiler/VM receipts were
authenticated before the trust table changed.

Exact Q `56f107fa44bf65c3df043efd3b784e7dd71484a5` ran
`bun scripts/agent-android.mjs build duo-lab` from the Exact checkout on the
Linux builder. That invoked `cargo build --profile host-dev -p duo-lab-linux
--target aarch64-linux-android` using Rust/Cargo 1.97 and the installed Android
target. Its v5 tag and all eleven pin mappings matched upstream exactly. The
legacy Exact Android cache stayed empty: this build consumed the published
bundle through the normal resolver.

The output `target/aarch64-linux-android/host-dev/duo-lab-linux` is an
AArch64 ELF PIE executable with interpreter `/system/bin/linker64`,
78,809,152 bytes, SHA-256
`dec5e208fe21cb32340f603aa0514409414918f9eafad0e623ba6580cc66d98e`.
It defines 3,276 Hermes symbols, has no unresolved Hermes symbols, and needs
no `libc++_shared`. Its dynamic dependencies are Android's `libandroid`,
`liblog`, `libjnigraphics`, `libdl`, `libm` and `libc`. The build exited zero
and the Q checkout stayed clean. The consumer-release run retains
`exact-android.identity`, `exact-android-binary.txt`, `exact-android-build.log`
and the toolchain/pin comparison under its v5 qualification receipts.

This retires the local pin patch. It qualifies the Android executable link,
not device execution. Existing `scripts/hermes-android.mjs` local receipt flow
remains available.

## Remaining consumer integration seams

`ibex2`, `ibex2-sqlite`, `hermes-lean-sys` and `hermes-lean-sys-installer`
declare empty workspaces. This stops Cargo from discovering an enclosing checkout
when formatting excluded path dependencies or running the printed installer command
in a nested worktree. Preserve these boundaries when refreshing the vendored crates.
The installer's own workspace takes none of the root's profiles, so its manifest
optimizes dependencies in the dev profile as the root's does: unoptimized, sha2
takes about nine seconds to verify each bundle under `--check`, which setup and
`sdkReport` run per target.
`scripts/app-workspace.test.mjs` checks the installer's workspace with locked,
offline Cargo metadata in ordinary and nested checkout layouts.

`ibex2/build.rs` emits empty binding bytecode and hardening metadata with
`EXACT_JS_ENGINE=stub` or for watchOS. `hermes-lean-sys/build.rs` selects the
same engine-free composition, with no native link. The watch bake refuses
entries reaching TypeScript until LLP 1126.001 §5 Q3 passes; host-side bakers
still validate and link the receipted lean VM. These remain Exact-only;
paired `HERMESC_PATH` export is now generic upstream support.

`secrets/darwin_keychain.mm` selects the bundle's
`ExactKeychainAccessGroup` only when `ExactAppIdentifier` equals the Keychain
service. Exact's app and WidgetKit extension can therefore read the app's
granted secrets through their shared group without changing another Ibex
consumer's service. `host/apple/bundle.mjs` publishes the two values, and
`host/apple/widget-keychain.test.mjs` is the opt-in simulator integration
witness. This is Exact application policy and remains local.

On Unix, `AppDirectories::snapshot` pins only roots that already exist and
leaves the others absent for that invocation. A read-only widget refresh can
therefore run first-launch application logic without creating the app's data,
cache or temporary directories. `exact-js` selects it only for a snapshot;
`missing_snapshot_roots_do_not_prevent_activation_or_create_app_directories`
is the direct consumer test. This is LLP 1126 D8 application policy and
remains local.

Unix app roots open with `O_DIRECTORY | O_NOFOLLOW`, including snapshot
roots. A root substituted by a symlink before its first open is refused;
opened roots stay pinned. `tests/app_storage.rs` covers both acquisition modes
and pinning after replacement.

The standalone locks above remain local. Application policy stays in Exact:
request scheduling and byte budgets, whole-exchange deadlines, source grants,
document session identity, JSON result vocabulary, native image codec, and
storage continuation ownership. Optional HTTP metric-file logging moved from
the vendor tree into `host/executor/src/metrics.rs`; its asynchronous
callback records remain consumer-owned, and blocking/native comparisons use
upstream's explicit diagnostic. Portable workers reserve an additional upload
copy in the lane's byte budget; native Apple upload buffers are charged once.
Each portable worker owns its process slot until its actual thread exits.
Partially started pools cannot retry against still-owned slots; failed base
startup releases every unstarted portable reservation.

Borrowed timer task support does not install ambient timers in Exact. Its
production group selection remains `PURE | CRYPTO | ABORT` (plus the existing
platform Intl selection), and Exact's trusted prelude retains its refusal policy.

The existing Snapback consumer proves partitioned storage, durable operation
queues, reconnect and account retirement. There is no Snapback asset-upload API
in this checkout: the deterministic losing-cancellation output witness is Exact's
native storage adapter. No new Snapback media feature or qualification is claimed.

## Local patch 14, Exact's app interface: document watches and expected writes

`bindings/storage.d.ts` declares `fs.watch(path, {recursive})` and the
`options` argument of `writeFile`, `atomicWriteFile` and `appendFile`
(`{expect: {modifiedMs, size}}`), as exact2 LLP 1069.010.000 D1 and D2a give
them. Declarations only: no Ibex source runs either. Exact's prelude
(`js/src/prelude.js`) records a watch as an LLP 1016.002 topic and does an
expected write as a `stat`, a comparison and the write, queued together; the
web glue and Exact's Rust storage do the same. Replay the declarations on a
refresh.

## Local patch 15, Exact's kept documents: a selected file named as chosen

`stdlib/documents.rs` adds `DocumentLease::selected_file_named(directory,
leaf, name, guard)`: a selected file the guest path names `name` while its
leaf in the retained parent is `leaf`. `selected_file` is that with the two
equal. Exact keeps a chosen file across launches under the name it was
chosen by (exact2 LLP 1069.010.000 D4), and a Mac bookmark follows the file
when it is renamed, so the two differ after a rename. The provider still
decides what the name admits; Ibex still opens only `leaf`, refuses every
path beneath it, and refuses `rm` on it. After admission, `readdir` on the
selected file reports `ENOTDIR` and `mkdir` reports `EEXIST` without touching
the leaf, even if it is absent or replaced by a directory.
`bindings/storage.d.ts` declares
`fs.recentDocuments()` and `fs.forgetDocument(path)`, which Exact's prelude
answers from the host's kept list; no Ibex source runs either. Replay both on
a refresh.

## Local patch 16, bindings' bytecode read where it is linked

`bindings.rs` links each binding's `.hbc` and `HARDEN_BYTECODE` at a 16-byte
boundary (`aligned_bytecode!`), and `bindings/install.cc`'s `CompiledBytes`
lends such a span to Hermes instead of copying it, since Hermes keeps a
module's buffer for the runtime's life and the statics live as long; a span
not so aligned is still copied. `include/ibex2_jsi.h` says so at
`CompiledScript`. In euxy on iOS the copies were 81 KB of malloc at idle.
Replay on a refresh.

## Local patch 17, a host-task worker gives its stack back when idle

`pool.rs`: on Apple platforms a worker that finishes a job with none queued
releases its stack pages below its frame (`MADV_FREE_REUSABLE`). The two
workers kept through a quiet spell otherwise keep every page a job reached
dirty for the process's life: 80 KB in euxy on iOS after its launch's reads.
Replay on a refresh.

## Local patch 18, `copyFile` from a chosen document into an app file

`stdlib/documents.rs` adds `copy_document`, which `fs::run_with_documents`
runs for `copyFile` from a `doc:` path to an `app:` path (exact2 LLP 1069.010
D1, ruled 2026-10-26); every other two-path document operation is still
refused. It admits `fs.read` on the document, then
`AppDirectories::copy_in` admits `fs.write` on the destination, and only then
is the provider asked. The source opens as `readFile` opens it
(`app_fs::document_file`, Unix and Windows) and the host copies its bytes, so
no result envelope bounds them. Replay on a refresh.

## Local patch 26, `realpath` of a chosen document on a desktop

`stdlib/documents.rs` answers `realpath` of a `doc:` path on macOS, Linux and
Windows (exact2 LLP 1069.010 D1, amended 2026-11-04): the item is opened as
`stat` opens it, then the retained directory's current location
(`fcntl(F_GETPATH)`, `/proc/self/fd/<n>`, `GetFinalPathNameByHandleW` without
its `\\?\` prefix) is checked to hold that directory still (device and inode,
or volume and file index) and joined with the names beneath it. A removed
directory is `ENOENT`. Elsewhere the refusal is `NotSupportedError`, before
the provider is asked. Replay on a refresh.

## Local patch 19, an app file staged in pieces and published whole

`stdlib/app_fs_unix.rs` and `app_fs_windows.rs` add `AppDirectories::stage(grants,
path)` and `Staged` (`write_all`, `publish`), for `fetch(url, {exactBodyTo})` (exact2
LLP 1108 D6 R2, ruled 2026-10-26): the host writes a reply's body into an app file as it
arrives. `stage` admits `fs.write` on the path, refuses a missing parent or a target that
is not a regular file, and creates `.exact-<hex>.partial` beside the target through the
pinned parent. `publish` syncs it and renames it over the target; a `Staged` dropped
unpublished removes it. The stub for other platforms refuses. Replay on a refresh.

## Local patch 20, the `local-network` grant source

`grant.rs` adds `fetch.local-network` and `websocket.local-network` (exact2 LLP
1133.036 D5, ruled 2026-10-26), written `net.fetch local-network` and
`net.websocket local-network`, with the word `development` for loopback too. It
admits a host the URL writes as an RFC 1918, link-local or unique-local
address (an IPv4-mapped IPv6 address by its IPv4), a link-local IPv6 address,
or a `.local` name; never a name that merely resolves to one, and never the
cloud metadata service, 169.254.169.254 (its IPv4-mapped spelling included)
or `fd00:ec2::254`. exact2's `grants` crate holds the same rule, and its
`tests/parity.rs` holds the two to `host/web/tests/fixtures/grants.json`.
Replay on a refresh.

## Local patch 21, a host's check at every fetch hop and WebSocket open

`boundary.rs` adds `HopGate` and `HostError::NotAllowed { reason }` (shown
`NotAllowedError: <reason>`); `host.rs` gives `Fetch` and `WebSocket` an
optional gate (`with_gate`); `stdlib/fetch.rs` (`fetch_stream_gated`,
`admit_hop`), `fetch_async.rs` (`start_gated`) and `websocket.rs`
(`open_gated`) ask it after the grants admit each hop, a redirect's included,
and before anything is sent (exact2 LLP 1133.036 D6): exact2's Apple executor
refuses a tool's, a wake's or a package event's reach to the local network
while the person has not decided. Without a gate nothing changes. Replay on a
refresh.

## Local patch 23, an upload's bytes sent

`stdlib/fetch.rs` gives `Request` an optional `sent` hook (`Sent`), told
`(loaded, total)` as the body goes out, reaching `(total, total)` once it has
all gone (exact2 LLP 1016.000 D10, XMLHttpRequest's upload progress; patch 46
stops it at the bytes written when the reply comes first), and an optional start hook beside it. `fetch_stream_gated` tells
the start hook once, when `admit_hop` has passed the first hop's grant check
and local-network gate, so a request they refuse never started (the
embedder's `exact:sending`). The hook answers whether the upload may go out:
an embedder that has let it go answers `false`, which stops it as an abort
does, and the loop checks the signal again after it. So an embedder that
aborts an upload, then reads which uploads started, is never wrong about one.
A request upstream's portable transport sends again, on a new connection,
when a kept connection closes unanswered, rewinds the counted reader, so its
bytes sent are told again from zero, as Chrome resets an upload stream to
resend (`rustls_http.rs`'s `send`; tested in
`an_unanswered_request_on_a_kept_connection_is_sent_again_on_a_new_one`).
`transport/darwin_http.mm` records URLSession's `didSendBodyData` on the
exchange, and `ibex2_darwin_http_headers` takes the bytes the caller last saw,
returning 2 when they change before the headers, so `transport/darwin.rs`
tells the hook on the opening thread and waits again.
`transport/rustls_http.rs` sends such a body through a counted reader, with an
explicit `content-length`, and tells the hook after each write (patch 46). Without a hook
nothing changes. `fetch_stream_gated`, every request's hop loop, moves the
body into each hop's request and back instead of cloning it, so a request
holds one copy of its body while it goes out, the platform's aside.

Tests: `darwin_stream_tests.rs`
`an_uploads_bytes_sent_are_told_before_its_headers`, `stream_tests.rs`
`the_portable_transport_tells_an_uploads_bytes_sent` and `stdlib/fetch.rs`
`an_upload_let_go_as_it_starts_never_reaches_transport`,
which run with this crate's manifest and in no lane. exact2's workspace tests
the hooks through its executor: the start, and none for a request the grants
refuse, in `exact-apple`'s `an_upload_the_grants_refuse_never_starts`;
URLSession in `exact-apple`'s async-lane
`an_upload_through_urlsession_tells_its_progress_and_waits_out_a_slow_reply`;
and the portable transport in `exact-js`'s
`an_upload_maps_its_bytes_sent_as_they_go_then_its_reply`. Replay on a refresh.

## Local patch 24, Exact's desktop processes: the transport and file operations

For exact2 LLP 1133.007. `process/` starts a child in a process group of
its own with default signals and only its three pipes (`posix_spawn` on
macOS, `fork` and `execve` with `PR_SET_PDEATHSIG` on Linux), `argv[0]` as
the caller names it, the parent's pipe ends raising no `SIGPIPE` on Apple
platforms, and answers what the host's supervision needs: waiting,
readiness of a pipe, signalling a group and listing its members, a
process's start time and session, the boot's identity, the password
database's home directory and login shell, and a wait at the process's
exit (`wait_at_exit`). It moves the child's descriptors above 2 before
putting them in place, and `fork` runs with every signal blocked until the
child's handlers are defaults. A `PATH` lookup skips relative entries.
On macOS a group's member list uses libproc's PID count to size its buffer
and retries a full buffer, so growth cannot silently truncate the list.
Exit observation uses `waitid(WNOWAIT)`, leaving the leader's identity
reserved until the supervisor and quit finish signalling, then `wait`
reaps it. `try_observe_exit` polls without waiting or reaping. Group liveness
and member lists exclude zombies on both platforms; Linux checks a zombie
leader's remaining threads. Failed inspection leaves liveness unknown and
is treated as alive for signalling and escalation. On Linux, membership is
`getpgid`'s answer, so a process outside the group that `/proc`'s
`hidepid` makes unreadable does not fail the scan (`process/scan.rs`).
`signal_process_if` replaces `signal_process`: it signals a pid only if a
test holds for it, and on Linux holds the process by a pidfd across the
test, so a pid reused after the test is not signalled.
`stdlib/fs.rs` resolves `home:/` and `bundle:/` (the namespaces `grant.rs`
reads) against roots the host sets (`set_roots`), the bundle read only,
each path admitted as spelt and run from its root's retained descriptor
with every component opened `O_NOFOLLOW` (`app_fs_unix.rs` `run_below`,
which `app:/` paths share), so no link below a root is followed; a native
absolute path is run the same way, admitted as spelt by the deepest grant
line that admits it, that line reached from `/` at each operation, its
links followed only until a directory an `fs.write` line (native or
`home:`) or an app directory names, compared by device and inode
(`reach`), and every name below that opened `O_NOFOLLOW`; a link on the
way that the running user owns, or that sits in a directory the user
owns or can write, is refused, naming it (`made_by_this_user`), so only
links such as macOS's `/var` and `/tmp` are followed; a line whose last
name the walk finds is a regular file (`fstatat` on the walk's descriptor,
`AT_SYMLINK_NOFOLLOW`, after `ENOTDIR`) is refused as `denied`, saying a
native line names a directory and to replace the line with itself less
that name, printed as `PathPrefix` prints a target (the file's directory
when that name is a link to it; `NamesAFile`); `access` checks its leaf with `AT_SYMLINK_NOFOLLOW`;
`open_read` opens a path to read it the same way, for a child's stdin; and `stat`
reads `mode` and `isSymbolicLink`; `fs_more.rs` adds `lstat`, `readlink`,
`access`, `chmod`, `symlink`, `digest` and `flock`/`unlock`, host opcodes
122 to 129 in `host_opcodes.rs` and `boundary_abi.rs`, the methods in
`bindings/install.cc` and `storage.d.ts`; a lock moves to the
`RuntimeState` that asked, whose `unlock` alone releases it and whose
shutdown releases them all. The process lines are exact2's own and never
reach `grant.rs`. Bionic's `faccessat` refuses every flag, so on Android
`crate::AT_EACCESS` is none and `access` checks its leaf without
`AT_SYMLINK_NOFOLLOW`, after the `fstatat` that found it no link. On iOS and
tvOS the host roots `bundle:` at a folder holding only the app's declared
resource trees and leaves `home:` unset (`fs_namespaces_tests.rs` checks
that a root of that shape confines its paths). Replay on a refresh.

## Local patch 25, a database file held by one SQLite library at a time

For exact2 LLP 1154 D3: an embedder may link a second SQLite library beside
this one (a native module's), and two in one process are safe only while no
file is open in both, since closing any descriptor of a file drops every
POSIX lock the process holds on it (sqlite.org/howtocorrupt.html §2.2.1).
`ibex2-sqlite`'s `held.rs` keeps the process's registry: `SqliteProvider::open`
counts each connection's file before SQLite opens it and refuses one that
`reserve(path)` has reserved; `reserve` refuses a file a connection has open,
and its `Reservation` ends the reservation when dropped and says whether the
file `existed()`; a connection's count ends at a successful close or when it
drops. A file is one entry by its name under its canonical directory and by
its device and inode, so a case alias or a hard link is the same file: an
absent file is created empty (`O_EXCL`) under the registry's lock when its
entry is made, by either side, and an entry keeps the identity it learned
then for its life, matching by name only where an identity is unknown: the
holder's descriptor keeps the inode from reuse, and a name unlinked and made
again names another file. `stdlib/app_fs_unix.rs`
`AppDirectories::sqlite_path` checks that the database is a regular file with
`fstatat(AT_SYMLINK_NOFOLLOW)` beneath the pinned parent instead of opening
and closing it, so resolving a path, a refused one's included, never touches
another library's locks. exact2 tests both through `exact-js`
(`tests/it/native_storage.rs`). Replay on a refresh.

## Local patch 27, a refused statement names what was refused and why

`ibex2-sqlite`'s authorizer refuses what it did (`ATTACH`, `DETACH`,
virtual tables, `SAVEPOINT`, `BEGIN`/`COMMIT`/`ROLLBACK` outside the
provider's own transaction, `load_extension`, and every `PRAGMA` outside
`PRAGMAS`), and records what it refused and the rule on the preparing
thread; the error answers that in place of SQLite's bare "not authorized".
A failure in `transaction` names the statement by its place in the batch
and its SQL (the first 200 characters). Since that SQL is the app's, a
failure SQLite reports as busy or locked begins `SQLite EBUSY: ` and any
other `SQLite: `, and exact2's prelude reads the code from that prefix
alone. exact2's web worker (`host/web/storage-worker.js`) answers the same
words and sets the code from SQLite's result. Replay on a refresh.

## Local patch 28, a `net.websocket` line is `ws` or `wss`

`grant.rs` refuses a plain `net.websocket` line whose origin has another
scheme (`net.websocket https://example.com`), as its subdomain and
scheme-wide forms already were: no socket has that origin, so the line
admitted nothing. exact2's `grants` crate refuses the same lines, and the
shared corpus (`host/web/tests/fixtures/grants.json`) holds them. Replay on a
refresh.

## Local patch 29, a batch that checks foreign keys at commit

`stdlib::sqlite::Connection::transaction` takes a `ForeignKeys` mode, and
`Database::transaction_with` passes it; the JS binding's
`transaction(batch, { foreignKeys: "check" })` sends it as the operation's
second argument (`sqlite_abi`); `undefined` or `null` options are none,
and any other key or value is a `TypeError` (exact2's prelude gives a
binding's `TypeError` or `RangeError` the code `failed`). In `Check`, `ibex2-sqlite` turns enforcement
off (`SQLITE_DBCONFIG_ENABLE_FKEY`) before `BEGIN`, runs the batch with the
batch's own `PRAGMA foreign_keys` lines authorized as no-ops
(`Authorization::Ignore`), runs `PRAGMA main.foreign_key_check` and
`PRAGMA temp.foreign_key_check` before `COMMIT` (unqualified, the pragma
reads only `main`), stepping through the rows and keeping the three to
name, ordered by table (by code point), rowid and referenced table,
and rolls back on any, and turns enforcement
back on in a drop guard on every path; a connection that cannot have it
back is closed. The connection's lock is held throughout. exact2's web
worker does the same, and exact2's bake moves `typescriptRuntime` to 10 for
a module whose grants admit `sqlite.open`, as an older prelude drops the
option and the options may be data.
Replay on a refresh.

## Qualification boundaries

Exact's continuation snapshots the image work visible to its runtime owner turn.
It can cancel those captured jobs, including other jobs already visible in that
runtime; it cannot discover or cancel jobs admitted afterward. This is runtime-work
ownership, not cancellation correlated to one JavaScript Promise. A losing
cancellation waits outside shared locks for publication's actual result.

The ImageIO codec bounds source pixels and retained encoded output, and admits
decoded/intermediate surfaces using component depth, aligned rows, decoder
workspace and 25% native-workspace headroom within its 512 MiB scratch estimate.
Large RGBA16 cases that previously exceeded the job reservation are now refused
before pixel decode. LLP 1069.002's **Native image memory qualification,
2026-10-13** records the retained adversarial corpus, baseline/final RSS and
four-job cancellation/teardown evidence at Exact
`2dd657fd7d93176b6d8e924849ed85170beda67c`, from baseline `dfa6a26344cf`.
That lane is integrated as `1b797a773e80b579e55a33e19dade886bcc31c0a` and is
included in the `4fd19fb4e2` checks below. The decision keeps in-process ImageIO
for this app-local API. The estimate is not a proven cap on ImageIO's allocator,
peak process RSS or native-call duration; hard process containment would require
a separate codec service. The configured worker limit belongs to each work owner.

### Previous landing evidence

The adoption delivered as Exact `6dd91519c4292a10c7b6450abe9e082c7fcffdff`
and Ibex `595224eb9e6e2419a82b0622a7c710a9eceb09f6` used vendored source
`de11540` and the v4 bundle. Its focused qualification passed 108 real lean JS
integration tests, 22 native storage tests, 15 Fieldnotes mixed TypeScript/Rust
persistence tests, and the render suite. Real native HTTP
drain/redirect/deadline cases passed in both modes. The separate Snapback
workspace passed 15 real-server client tests, six durable-device tests and one
client unit test against its pinned server. The browser grant corpus passed
111 assertions. These receipts belong to that landing, not the newer source pin.

The macOS Fieldnotes app was built and driven with the named scratch store
`ibex-adoption-20261013`: save a note, write/read its Rust backup, restart,
delete the note, restore that file through TypeScript, then restart again.
The observed count changed 1 → 0 → 1, and the final title/body matched the
saved note with no pending work. This uses isolated test storage, not an
existing notebook. All six required Exact checks passed on `6dd91519c`;
the focused checks, app drive, final checks and production lean phase diagnostic
retain their individual revisions in the local consumer-adoption run ledger.

### Historical v4 follow-up evidence

Exact `4fd19fb4e2d5bdad1311a18a694feff9b6f1efc3`, with vendored source
`b20dded` and the v4 bundle, passed all six required checks: build, tests,
clippy plus fmt, caps, boot, and the full workspace check. Its additional
`exact-data-host` run passed 22/22 tests. The signed macOS Conduit fixture,
using isolated identity `com.ccheever.ibexasyncqualification` and scratch
storage, was built with `EXACT_HTTP_ASYNC=1` and driven through
home → article → home → login. Four articles loaded, the article body was
present, and every pending queue was empty. The app remained signed out and
issued no public writes. Four recorded HTTP transactions used h2 with one
observed endpoint pair. This is functional reuse evidence, not a timing result.
The consumer-release run retains `exact-4fd-six-gates.json`,
`exact-4fd-conduit-macos-receipt.json` and their command logs.

The rustdoc-only refresh to `bf74e54` passed formatting, caps, both standalone
locked/offline metadata checks, installer/root lock overlap and the vendor
comparison.

### Published v5 qualification

Exact `56f107fa44bf65c3df043efd3b784e7dd71484a5` (Q) consumes upstream
`e60b3e0` and the published v5 bundle. Q includes the accepted mainline through
`e8b90ef40`; all results in this section used that clean source revision.
The explicit installer acquired the authenticated bundle, and its offline
`--check` passed before compilation. The macOS archive digest is
`bc71c7b398f1d41a5390761b75ce72e7be234aa48cb1ddec947224188ed6a4f2`.

All six required Exact checks passed on Q, with 22/22 additional native
storage tests. The semantic comparison selected 298 corpus cases, 40 generated
cases and eight lowering cases: no divergence. The eight lowering cases covered
1,203 agreeing runs; seven bodies were outside the modeled fragment.

The signed Conduit fixture was rebuilt with v5 and `EXACT_HTTP_ASYNC=1`.
Its native Mac drive again loaded four articles, opened a nonempty article,
returned home and reached login. It stayed signed out with no pending work or
public writes. Four lane transactions used h2 on one observed endpoint pair;
no timing or LTE qualification is inferred. The executable SHA-256 is
`f7c169d57dd6a35c8ae25cc2d87987aba81d300e33790a1b948cb1be3c645ca6`.
The native resolver outputs name the v5 cache, and the owned app process exited.

Native x86_64 Windows Q also installed and verified v5 offline. Its archive
digest is `db9233d08c25a33d7b1cdd2d0eb9428e98020fa6de7ab396b0adfb241b61d1e0`;
receipt schema `ibex/hermes-upstream-pinned-receipt/2` has SHA-256
`32c11e970e9783fc8f4b5171700e146c0146a17ad7593a8237c70a3f9b34903e`.
The locked/offline run passed 25 data-host tests, two JS library tests and
101 JS integration tests, with one manual startup diagnostic ignored. The
ordinary TypeScript app rebuilt and its platform-delivered button presses
changed the Hermes-backed resource 0 → 1 → 2. Its EXE SHA-256 is
`81d1932eb51ee3e36410370cddf6ece5e272f3837788f54f349b85d0a3d3ac61`.
No app or build process remained. The Windows render-host HTTP compilation
boundary recorded above remains open; these results do not qualify that host.

The consumer-release run retains `exact-56f107fa4-six-gates.json`,
`exact-56f107fa4-conduit-macos-receipt.json`, `windows-v5-ledger.json`, the
Android receipt named in row 8, and their logs. Codex and Grok independently
cleared Q's code and pin adoption. Later evidence-only commits preserve Q's
runtime and native proof attribution; their required gate receipts name their
own clean revisions.

### Windows render-host HTTP follow-up — 2026-10-14

The completed follow-up at Exact
`b195e96dc953733589e54167286da92f59725389`, with current main
`0d2d2e28e972ae5bf603105c794b45fecc6a2b08` merged and the vendored Ibex pin
`e60b3e0570315f31710744c1187f3cec5574fedf`, closes the current Windows
render-host boundary without changing Q's historical receipts. Windows now
uses a private loopback TCP wake pair with WinSock `WSAPoll`; Unix keeps its
existing `UnixStream` and `poll` path. Pending wake state clears only after a
byte is consumed, and wake-channel failure releases queued and held sockets.
Both platforms retain LLP 1048.000 D10's 2-second per-socket hold, 64 KiB read
cap, 256 held-connection cap, bounded inbox, and immediate half-close. The
server's separate `Stopper` waits up to twice the render deadline plus
10 seconds for render workers, then drains the closing thread's remaining
2-second socket holds.

On native Windows, the focused real loopback executor test passed, followed by
118 unit tests, 37 integration tests, and one look test, with nine ignored and
no failures. All six focused linger tests, all-target clippy, and caps passed.
With `EXACT_HTTP_ASYNC` unset, the directly built Hermes-backed Weatherlight
renderer served its generated plan over loopback: health and `/` returned 200,
and `/` carried 100,584 bytes of rendered Weatherlight HTML. The standard web
build's existing colour-warning gate failure remains a retained failed attempt,
not a whole-app build pass. This run reused the authenticated v5 receipt and
archive hashes recorded above.

An independent native Linux run of the same corrected source passed 120 unit
tests, 37 integration tests, and one look test, with nine ignored and no
failures; all six linger tests and all-target, all-feature clippy passed. The
source-attributed commands, hashes, superseded first implementation, retained
failures, and app drive are recorded in the
[closed Windows HTTP issue](../../issues/closed/20261013-exact-render-windows-http.md).

### Physical-device evidence and remaining rollout boundaries

Six bounded ImageIO/ImageJob cases passed on a physical iPhone 17 Pro Max running
iOS 27.0.1 (24A446), using production codec and vendored job code from Exact
`a01c62241440300f3644ecde83592606262c58e7`. They cover four concurrent image
transforms, RGBA16 input, predecode refusal, cancellation, owner close, and
cancellation after commit starts. The largest observed process resident peak
was 195,837,952 bytes; sampled physical footprint peaked at 118,916,840 bytes.
LLP 1069.002 records the fixtures,
source hashes, retained receipts and limits. This finite workload does not prove
a whole-app memory limit or behavior under memory pressure.

The async-enabled iPhone app launched, but its control connection could not be
established. Rebuilding the scratch fixture with the documented local-network
permission setting reached a connection timeout. Network navigation and
same-store restart interaction remain unqualified pending device connectivity
and consent; the image results above do not substitute for them.

The isolated arm64 Android Shelf APK also built with the authenticated v5
bundle from Exact `a01c62241440300f3644ecde83592606262c58e7`. Its SHA-256 is
`2dc2aa1ed32f146d77e79962ec5b6b04611e30ad7affa68c4fe4a676c66f5588`;
it requires Android API 31 or newer. No physical Android device was
reachable for installation and interaction in this follow-up, so this remains
build evidence only.

Charlie assigned the historical bsky4 rollout campaign to the agent on Tuft;
LLP 1120 §5 records the original baseline and remaining measured gate. Async
remains off by default. Independent stage-1 code review, controlled TLS
setup-stall diagnostics, Mac smoke, and the bounded image tests do not replace
that campaign.

## Local patch 29, assignments over inherited intrinsic properties

`crates/ibex2/src/bindings/harden.js` replaces configurable, writable prototype
properties with accessors before freezing. Assigning on an inheriting object
creates its own property; the intrinsic stays fixed. SQLite verifies its captured
identities before this trusted transformation, then pins the resulting accessors.
The adapter's value conversions, integrity checks and bootstrap primitives live in
separate internal headers to keep each source under 1,500 lines.
Exact's `js/tests/it/pure.rs` covers shadowing and the frozen descriptors;
its storage tests exercise the SQLite integrity check after hardening.

## Local patch 30, SQLite provider registration by the app shell

`stdlib::sqlite` holds an optional process provider. `ibex2_sqlite::link`
installs it before any storage session is created. Exact's generated native
entry calls it only when its composition names `sqlite`; the TypeScript and
Rust storage executors read the same slot. File reservations still share the
provider's registry. SQL grants and the manifest's link-ahead select it;
development and unknown compositions keep it.

An absent provider in `sqlite_abi` (JavaScript) or `host::Sqlite` (Rust)
refuses an admitted open with `Unlinked("sqlite")`, matching Exact's portable
storage executor and bake. Replay these refusals on a refresh.

## Local patch 31, a fetch reports each hop's response head

`stdlib::fetch::Request` carries an optional `received` callback. The blocking
fetch (`stdlib/fetch.rs`), the asynchronous one (`stdlib/fetch_async.rs`) and
the portable transport (`transport/rustls_http.rs`) call it with a hop's
method, URL, status and headers when that hop's response head arrives, before
a redirect is followed or a body is read. Exact's executor uses it to drop
cached replies a write may have changed (RFC 9111 §4.4; LLP 1162 D4): a write
that a redirect follows, or whose body then fails, has still been answered.
Unset, nothing changes.

The Darwin transport's wait for a head (`ibex2_darwin_http_headers`,
`transport/darwin_http.mm`) hands back a head that arrived together with a
failure: one refused for its declared length, or one whose exchange failed
before the wait returned. `DarwinTransport::open` tells it and returns the
failure as before. URLSession tells a head only once some of its body, or
the connection's end, has come.

The asynchronous fetch's blocking adapter (`run_blocking`) passes a head the
transport told with a failure to `Sink::observed`, which `HopSink` reports as
it reports `head`. Both call the embedder outside the task's lock, since what
it calls may abort the fetch.

Replay all of it on a refresh.

## Local patch 32, gated WebSocket subprotocols

The host binding passes protocol names through its existing network gate
(LLP 1016.000). It checks the selected name against the offer, including the
browser rule that an offer requires a selection. Protocol names are case-sensitive,
including duplicate detection in the JavaScript constructor. Gated opens, receive-only
opens and event watches share the selection check; a refused watch reports error
then close 1006, without open. `stdlib/websocket.rs` tests those entry points.
`host/executor/src/protocol_tests.rs` exercises
URLSession and the portable transport against a local handshake server.

## Local patch 33, an asynchronous fetch's reply body destination

`stdlib::fetch::Request::response_body` optionally names a `BodyDestination`
and an error-body allowance. `FetchTask` sends only the final 2xx body's
chunks there and calls `finish` for its response metadata, outside its lock.
The transport's serial sink context calls both: on the bounded blocking
adapter, its worker thread may write a file; a native delegate queue must
arrange a different execution context before using a disk destination.
Other statuses collect at most the error allowance, and redirects skip their
bodies. Abort retains the destination through the physical end; its last
release cleans up any unpublished file. Unset, nothing changes.

`Staged::publish_if` syncs before asking its caller to admit the rename.
Exact uses that boundary to arbitrate abort and deadline against publication
(LLP 1162 D1). The existing `publish` admits unconditionally.

Replay both on a refresh.

## Local patch 34, asynchronous transport retirement

`AsyncTransport::retire` releases a lane's idle resources while accepted
exchanges retain their physical owners (LLP 1162 D8). `BlockingAdapter`
forwards retirement to its `WorkerService`. Exact's service closes its
bounded pool; reserved and running jobs keep their workers until they return.
The bounded pool also supports retirement while its service handle is retained.
`DarwinLane` serializes preparation against retirement and invalidates its
session with `invalidateAndCancel`, even when callers retain the lane.
Retirement refuses further preparation and task completions still arrive.

Replay these lifecycle methods and the retained-pool and lane tests on a refresh.

## Local patch 35, a redirect's next hop names its destination

The asynchronous fetch tells its embedder that a redirect's next hop is
ready (`next_hop`). The call now carries the hop's URL, so the embedder
admits the hop against its destination's origin before it starts it, as it
admits a first hop (LLP 1162 D8). An embedder that ignores the URL behaves
as before.

Replay the parameter and its test on a refresh.

## Local patch 36, a fetch binding under a narrower grant set

`Fetch::with_grants` is the binding's transport and gate under another
`GrantSet`. Exact's native executor runs a request that carries a source's
narrower scope (`exact_data::Mixed` sets one on each of its sources'
requests) on the worker's or lane's own transport with the scope's grants,
so it reuses the connections that transport keeps rather than opening its
own. Replay on a refresh.

## Local patch 37, a speculative fetch

`stdlib::fetch::Request` carries `speculative`. `stdlib::fetch_async::HopRequest`
keeps it on every redirect hop, and `run_blocking` passes it to the blocking
transport. Both Darwin transports (`transport/darwin.rs`,
`transport/darwin_async.rs` and `darwin_task.mm`) send such a request with
`allowsConstrainedNetworkAccess` false, so URLSession refuses it under Low
Data Mode; the portable transport ignores the field.
Exact sets it on a preload, a request made before the app asks for it
(LLP 1162.000 D4, LLP 1162 D7). Unset, nothing changes. Replay it on a refresh.

## Local patch 38, rusqlite 0.41

`ibex2-sqlite` requires rusqlite 0.41.1, whose `libsqlite3-sys` 0.39.1
bundles SQLite 3.54.1: earlier SQLite reads uninitialized page-cache memory
through a WAL whose page size differs from its database's
(RUSTSEC-2026-0459). rusqlite 0.41 returns `column_names()` as an iterator,
takes `&mut` to install the authorizer, and reports a failed prepare as
`SqlInputError`, whose text appends the whole SQL and an offset;
`sql_error` reads SQLite's message from it, so a refusal and a batch's
shortened statement read as before. Replay on a refresh.

## Local patch 39, live URLSearchParams iteration parsed once per change

`bindings/url.js` iterates as WebIDL's pair iterators do: each `next()`
reads the pair at its index in the list as the list is then, and
`forEach` likewise. The parsed pairs are kept until the query changes (a
view's URL replaces its components on every change; a list of its own
drops them on a write), so iterating an unchanged query of N pairs parses
it once rather than N times. Exact's `js/src/pure.js` wrapper, which made
iteration live by taking a snapshot per step, is gone.
`js/tests/it/pure.rs` checks mutation during iteration against Chrome and
the scaling. Replay on a refresh unless upstream iterates live.

## Local patch 40, a header list's sorted order kept between indexed reads

`Headers` keeps its entries' indexes in name order once an indexed read
asks (`Headers::sorted_entry`), and drops them when a header is added or
deleted; a changed value keeps its place. `bindings/headers_ops.rs` reads
an iteration step's name or value through it, where it copied and sorted
the whole list for each, so iterating N headers sorted them 2N times.
`PartialEq` compares the entries alone. `stdlib/fetch.rs` tests the reads
across changes; `js/tests/it/pure.rs` checks the scaling and, against
Chrome, mutation during iteration. Replay on a refresh.

## Local patch 41, query data keeps a `?` that begins a name

`stdlib/url.rs`'s `SearchParams::parse` no longer removes a leading `?`.
Its input is query data: `bindings/url.js` removes the one delimiter the
URL Standard's constructor removes, and a URL's query excludes its own, so
a second removal made `??a=1` the name `a` rather than `?a`. `url.rs`
tests the parse; `js/tests/it/pure.rs` compares construction, a URL's
view, lookup and mutation with Chrome. Replay on a refresh.

## Local patch 42, an app folder read or copied as a file is `EISDIR`

`stdlib/app_fs_unix.rs` refuses `readFile` of an `app:/` folder, and
`copyFile` from one, with `EISDIR` (os error 21), as a write to one already
is and as the web's store refuses all three, so the app's `code` is the same
on Unix hosts as on the web. Windows still answers `failed` for these
(`windows_directory.rs` refuses with no filesystem code), and the test
asserts the folder codes only under `cfg(unix)`. `copy` checks its source before it opens the target, so a
folder's copy no longer leaves an empty file behind. `js/tests/it/storage.rs`
checks both codes and that no file is made. Replay on a refresh unless
upstream refuses folders so.

## Local patch 43, bundled SQLite on Apple

`ibex2-sqlite` bundles SQLite 3.54.1 on Apple too: the minimum supported OS
libraries are older than rusqlite's 3.45.3 floor (#565). Exact's Apple Cargo
driver sets `SQLITE_ENABLE_LOCKING_STYLE=0` and renames `fstatfs` to
`exact_sqlite_fstatfs` for iOS, tvOS and watchOS. `src/apple.c`, compiled by
`build.rs` under the same SDK declaration, returns a zero-filled `statfs` and
success. This removes the FAT/exFAT inode workaround and filesystem-specific
locking, including proxy locks, for app-container storage. macOS app builds
retain SQLite's defaults. The replacement is available to Mac build tools too,
since `LIBSQLITE3_FLAGS` also reaches an iOS build's host dependencies.

Recheck every filesystem probe when updating the amalgamation. Replay the
bundled dependency, C replacement and build driver together on a refresh.

## Local patch 44, the events binding's engine hooks for an embedder

`InstallOptions::events_hooks` names a global through which the installer
publishes the events binding's engine hooks (`reportException`, `onUnhandled`,
`onHandled`, `configure`), frozen, as it publishes the other bootstrap outputs.
Exact's data module reads them before its code runs (exact2 LLP 1175).
Installed so, the binding reads no clock, since that realm refuses one to its
code: `Event.timeStamp` is 0. `configure({ report, result, during })`, allowed
once, makes the embedder hear every report with whether a listener cancelled
it, in place of `nativeReport`; hear each listener's and `on…` handler's
return value; and run the host's own dispatches of `error`,
`unhandledrejection` and `rejectionhandled` inside `during`. `reportException`
and `onUnhandled` return whether a listener cancelled. Without the option the
binding behaves as upstream's. `js/tests/it/error_events.rs` covers it. Offer
upstream; replay on a refresh until it lands.

## Local patch 45, ibex2 takes Exact's private copy of ureq

`ibex2` depends on `ureq = { package = "exact-ureq", path = "../../../ureq" }`,
ureq 3.4.0 vendored under another name (`vendor/ureq/EXACT-PATCHES.md`): the
portable transport implements ureq's `unversioned` traits, which change
between minor releases, and an app's own `ureq` must resolve apart from it
(LLP 1153.003 D6.5). `hermes-lean-sys` and its installer use only ureq's
stable API (an agent and its TLS roots, local patch 47) and require `"3"`. The atomics call `try_update`, Rust's current name for the
deprecated `fetch_update`. Replay on a refresh.

## Local patch 46, a request body streamed from a file

`Request::body_file` is a body read from a file as it goes out (exact2 LLP
1133.008.000 D4): a `BodyFile` over the embedder's `FileBody`, which gives the
body's length and a reader from its first byte for each attempt. A redirect
that drops the body drops it; a request with both it and `body` is refused, as
is one on the asynchronous fetch. Its length is the `Content-Length`.

The portable transport sends it through `SendBody::from_reader`. A request
whose body is a file and which names no timeout has no limit on the whole
exchange: each write may wait 60 s (`IDLE_SEND`), then the reply has its usual
15 s and 15 s. A resend on a new connection opens a new reader. Progress, for
any counted body, is told after each write that succeeds (`Written`), never on
the read before it, and is no longer filled to `(total, total)` when the
response arrives: a reply before the whole body went out tells only the bytes
written.

Darwin sends it as `uploadTaskWithStreamedRequest:`. `needNewBodyStream`
supplies each attempt's stream, the first included: the input half of a 64 KiB
bound stream pair, with a thread (`Ibex2FilePump`) copying the reader's bytes
into the output half and never writing when there is no space. URLSession's
own stream lets it read a reply that comes before the body has gone (a 413 in
under a second, measured); an `NSInputStream` subclass left it waiting 60 s for
its timeout. A reader that fails fails the exchange with its words unless a
reply came first. Progress stops at `didSendBodyData`'s last count.

Replay on a refresh.

## Terminal heap exhaustion

The borrowed adapter propagates native engine exceptions through installation,
checkpoints and task delivery. These changes adopt the consumer contract from
upstream `6ce0bf09495cee43fdc9fb2ee00ca5520796d2ef`: Hermes heap OOM requires
immediate adapter and runtime destruction, without JavaScript, job draining or
collection. Exact's shim recognizes that exception only when the bundle receipt
enables exception-on-OOM and RTTI. The pinned v7 release enables both.
Exact's heap tests exercise retirement and replacement with the published bundle,
including timers, stream mappers, worker placement and in-flight storage.

## Socket close confirmation

`MessageSender::wait_close` distinguishes submission from completion. URLSession
confirms receipt of the peer's closing frame through its delegate; the portable
pump confirms that the close frame reached TCP. The native executor waits across
this boundary before a suspending host returns, with a bounded deadline. Ordinary
resource releases wait on a worker. Replay on a refresh.

## Session-free file transfers

`Fetch::admit` checks the binding's grants and hop gate without sending a
request; a later `stream` checks them again at every hop. HTTP admission
refuses non-HTTP schemes and, at the first hop and at every redirect, a URL
with userinfo: Fetch throws a `TypeError` for a
[request URL that includes credentials](https://fetch.spec.whatwg.org/#dom-request)
and answers a network error for a
[redirect to one](https://fetch.spec.whatwg.org/#http-redirect-fetch) in
another origin, which every server is to an app. The background-transfer
carrier refuses userinfo in its own hop gate as well, beside the source's
grant and gate. Redirects rewrite POST on 301/302,
rewrite non-GET/HEAD on 303, and drop body headers when dropping the body,
as Fetch's redirect algorithm requires. Authorization remains removed after
any origin change.
Redirect credential stripping compares parsed URL origins, leaving hostless
and non-HTTP targets to hop admission. Such targets fail URL validation,
not HTTP-origin extraction, so a carrier never retries them as network faults.

`Request::idle_timeout` lets a portable file transfer outlive the ordinary
total deadline while bounding DNS (including the wait for a lookup helper)
and each socket wait. An explicit total timeout
wins; a reused connection restores the next request's own timeouts. TLS,
trust selection and HTTP framing are unchanged. The Android carrier uses
this through the same endowed fetch binding as ordinary requests (LLP
1133.017). Replay on a refresh.

`AppDirectories::publish_file` admits a destination under pinned directories and
renames a completed host-owned file into it on the same filesystem. Recovery
recognizes an already-published destination; storage errors preserve the source
for retry. This avoids requiring another response-sized allocation at publication.

## Windows known folders

LLP 1173 D4's create-only namespaces use an OS-selected root, UNC included,
and retain handles below it. Their walk refuses name-surrogate reparse tags
and reopens admitted directories with normal reparse processing so cloud
filters can recall placeholders: an `NtCreateFile` with an empty name relative
to the held handle, since `ReOpenFile` refuses directories. The reopened handle
must identify the same object; tags are checked again before use. Every
directory it opens is reopened because Windows can hide cloud tags; an entry it
has just created with `FILE_CREATE` is not, having nothing to recall, unless it
already carries a tag. Such an entry is held with `DELETE`, and removed if that
check fails. The app and native drive walks keep refusing all reparse points. Leaf validation shares the native path rule. Exclusive
creation uses `FILE_CREATE`; atomic publication uses the retained parent and
`FileRenameInformation` with `ReplaceIfExists` false. Known-folder `mkdir`
creates every component exclusively, with `FILE_CREATE` on Windows and
`mkdirat` on Unix; app walks keep their open-or-create behavior. On Windows a
failed `mkdir` removes the directories it created before the failing one,
through their own handles, and names any it could not remove. Replay on a refresh.

The portable tag and name tests run on macOS. Windows tests cover collisions,
nested `mkdir`, retained parents and agent roots. Native qualification remains pending:
`cargo run -p exact-windows --example known-folders` probes OneDrive and UNC
redirection before a Windows app drive claims the cell. Append
`-- pictures OnlineOnly` to exercise an existing dehydrated child directory.

## Local patch 47, the Hermes download trusts `SSL_CERT_FILE`

`build_support.rs` downloads a pinned bundle with Mozilla's roots, or, when
`SSL_CERT_FILE` names a PEM file, with that file's certificates in their place,
as OpenSSL and curl read the variable. `exact setup` then works behind a
TLS-intercepting proxy whose CA is private. The pinned SHA-256 still decides
what is extracted. Replay on a refresh.

## Content codings off Apple

The rustls transport builds ureq with `gzip` and `brotli`: a request names
`Accept-Encoding: gzip, br`, and a response with that `Content-Encoding` reaches
`fetch` decoded, as NSURLSession gives it on Apple and Fetch requires
([HTTP-network fetch](https://fetch.spec.whatwg.org/#http-network-fetch)). A server may
send a stored coding unasked: Cloud Storage serves a `br` object to a request that
names only `identity`. Without the features Android, Linux and Windows handed the
coded bytes to `text()`. Replay on a refresh.

## Local patch 48, an open-ended response

`Request::open_ended` marks a response its caller reads for as long as it
holds it: exact2's event streams and byte streams (LLP 1016.000). Without an
explicit `timeout`, no limit of the transport's own ends it, on the whole
exchange or on a quiet body, whatever its status. An explicit timeout keeps
its meaning, and ordinary requests keep their limits.

The portable transport (`transport/rustls_http.rs`) keeps its limits on
connecting (10 s) and on the head (15 s), and lifts every read deadline ureq
would apply to the body (its 30 s global and 15 s body limits) once the head
arrives, for an `open_ended` request and for a successful response whose
last `Content-Type` is `text/event-stream`. Cancellation still interrupts a
blocked read, and a pooled agent restores its deadlines when it is leased
again. `transport/rustls_timeout_tests.rs` probes the deadlines passed to the
wire, default and expired, and agent reuse; `transport/stream_tests.rs`
exercises later events, explicit timeout expiry, cancellation and release
against loopback peers.

Darwin (`transport/darwin_http.mm`) sets an open-ended request's
`timeoutInterval`, which ends a task after that long without data (60 s by
default), to a limit nothing reaches (`Ibex2Unbounded`). The head goes
without a limit too: URLSession calls `didReceiveResponse` only with the
first body bytes or the body's end (content sniffing; measured for
`text/event-stream`, `application/octet-stream` and `text/plain`), so a
server that answers and then waits shows no head until it sends. Its
session's `timeoutIntervalForResource` (seven days by default) is the same
unreached limit; that limit is a session's, so the pool leases an open-ended
exchange only a session of its kind and every other request one that keeps
the seven days, and at its four sessions an idle one of the other kind gives
up its place.

exact2's `host/executor/src/stream_quiet_tests.rs` holds an event stream and
a byte stream quiet past each transport's limits against a loopback peer, on
rustls and on URLSession. Replay on a refresh.

## Local patch 49, one connection pool a transport

exact2 runs its independent lane's six workers, its scoped requests and its
launch preloads over one transport (LLP 1162 D5), so its reads share their
connections.

Darwin (`transport/darwin_http.mm`) ran each exchange on one of four
sessions leased to it alone, because its delegate waits while the exchange's
reader is behind, which would stall every task of a shared session; each
session had its own connection pool. `Request::collected` says the caller
reads the reply to its end as it arrives, into memory (exact2's plain reads
and preloads, not a reply written to a file). Such an exchange, if not
open-ended and its limit at most 64 MiB, a plain request's ceiling, now runs
on one `shared` session per transport, whose delegate waits only while the
reply is a mebibyte ahead of its reader, which a collecting reader never is
for long: the session's other exchanges are not held, and what it holds of
each reply stays bounded. The reader takes the reply from an offset and drops
what it has taken once that is half the buffer. URLSession task suspension is
no substitute: suspended in `didReceiveData`, a loopback task kept delivering
its whole 8 MiB. The shared session's `HTTPMaximumConnectionsPerHost` is 32:
URLSession's six (macOS) or four (iOS) would queue a read behind replies its
origin holds, which sessions of their own never did, and 32 is more than
exact2 runs at once on one transport (six workers, eight launch preloads,
eight prefetches). HTTP/2 and HTTP/3
carry an origin's exchanges on one connection. Any other exchange still
leases a session of its own (local patch 48's kinds), so a stream, an unread
reply or a slow file write never holds back the shared session.
`transport/darwin_stream_tests.rs` has eight collected exchanges at once take
their heads, which the peer sends only once all eight are in, beside four
open-ended ones, and eight more after them open no connection.

The portable transport (`transport/rustls_http.rs`) leases each exchange an
idle agent of its own, each agent with its own pool. A lease now takes the
newest idle agent whose last requests went to the request's origin (the
eight it remembers), so a request finds a connection another left open to
it. Replay on a refresh.

## Local patch 50, a URL the web would encode

`fetch("…&details->>status=eq.open")` reaches a browser's server with `>`
percent-encoded: the URL Standard's parser encodes what a query, a path or a
fragment may not hold as written. The portable transport parses a URI and
refused the request, "invalid uri character", so one line of app code worked
on the web and Apple and failed on Android, Linux and Windows. `prepare_body`,
which both the blocking and the asynchronous fetch pass a request through,
replaces a URL that holds a character RFC 3986 gives no URI with the `url`
crate's serialization of it. A URL that is already a URI is sent as written,
so no request that worked changes. Replay on a refresh
(`stdlib/fetch.rs`, `a_url_the_web_would_encode_is_sent_encoded`).

## Local patch 51, an execution says whether it changed the database

exact2 tells an app's other windows of a SQLite write (exact2 LLP 1030.004
D2) when the statement changed the database, a row or the schema, so a
`DROP` or an `ALTER` counts and a `CREATE TABLE IF NOT EXISTS` that finds
its table does not. `stdlib::sqlite::ExecuteResult` gains `changed`:
`ibex2-sqlite`'s `execute` reads the connection's `total_changes` (which
moves for a trigger's rows too) and `PRAGMA schema_version` before and
after the statement, and `changed` is either moving. The authorizer admits
`PRAGMA schema_version` only while the provider reads it, as it admits the
provider's own `BEGIN` and `COMMIT`. `changes` is zero when the total did
not move: `sqlite3_changes` keeps the last `INSERT`, `UPDATE` or `DELETE`'s
count after any other statement (https://www.sqlite.org/c3ref/changes.html).
`sqlite_abi` result field 9 carries `changed`, and the JS binding's
execution has it beside `changes` and `lastInsertRowid`; exact2's prelude
reads it and gives the app the other two, so `storage.d.ts` leaves it out.
exact2's web worker answers alike. `src/tests.rs` covers each statement
kind, and exact2's `exact-js` `topics` test what announces. Replay on a
refresh.

## Local patch 52, `llvm-lipo` off a Mac

`build_support.rs` selects Xcode's `lipo` on a Mac and `llvm-lipo` off a Mac to
inspect and thin Apple Simulator archives. Off a Mac it looks in
`EXACT_APPLE_LLVM_BIN` when set and nonempty, otherwise on `PATH`. Launch errors
name the executable path tried and give the remedy for the missing tool. Failed
`-archs` and `-thin` commands name the executable that ran. `build.rs` emits
`cargo:rerun-if-env-changed=EXACT_APPLE_LLVM_BIN` only when linking Hermes for a
Simulator off a Mac. `tests/build_support.rs` covers tool selection, archive staging
off a Mac, and the launch and command errors. Replay on a refresh.

## Windows recursive removal

On filesystems that support POSIX disposition, a reader can keep its open
handle while `fs.rm` removes the file's name and its parent directory.
A recursive removal visits the other entries even when one is locked. It
retries a sharing violation or a nonempty directory for five short intervals
before returning the Windows error (expo/exact#1096).

## Local storage without a network transport

`Host::without_network` keeps the stores, filesystem providers and grants of
a local consumer. Its HTTP and WebSocket bindings report the capability as
unavailable. Exact gives this host to its Hermes storage context. The runner
executes the app’s network work on a separate host with its own transports.

## Standalone HTTP binding

`Fetch::new` binds grants to a supplied HTTP transport. Exact's driver uses
it for a fixture response without constructing a WebSocket client or stores.
