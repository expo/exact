# Recipe: a GPU module that calls into Java on Android

A surface whose crates call into Java (HTTP through `HttpURLConnection`, the app's `AssetManager`,
a crate that reads `ndk-context`) needs the app's Java VM and `Context` before its first request.
Android opens a GPU module (recipe `gpu-module`) with `dlopen`, which runs no `JNI_OnLoad`, so the
module asks for them with one line beside `module!`:

```rust
exact_gpu::android_init!(|android| unsafe {
    mln_storage::android::init(android.env, android.context)
});
```

`android` is an `exact_gpu::android::Android`, three `*mut c_void`:

- `vm`: the process's `JavaVM*`. Attach a thread the module starts with it.
- `env`: the calling thread's `JNIEnv*`, valid for this call only.
- `context`: the application `Context`, a global reference valid on every attached thread for the
  life of the process.

The expression returns a `Result` whose error is `Display`; compose several initializers with `?`.
A crate that reads `ndk-context` is given both by
`ndk_context::initialize_android_context(android.vm, android.context)`.

- The host calls it once in a process for each module, after checking the module's baked digest
  and before anything else in the module runs: before a device and before any surface.
- It may run on a thread the host attached to the VM, where `FindClass` sees only the system class
  loader ([JNI tips](https://developer.android.com/ndk/guides/jni-tips#faq:-why-didnt-findclass-find-my-class)):
  load an app's own class through `context.getClassLoader()`.
- It is not `JNI_OnLoad`: the VM did not load the library, so bind a Java class's `native` methods
  with `RegisterNatives`.
- An error, or a Java exception left pending, refuses the module: its canvases stay unpainted and
  the log says `GPU module …: Android initialization failed:` and the error.
- The line needs no `cfg`: on every other platform, the web included, it compiles to nothing. A
  module without it is opened as before.
