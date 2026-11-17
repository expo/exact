# The library's JNI binds `dev.exact.widgets.Native` by name (`exact_painted::widgets_jni!`), and each app's providers
# subclass `Provider` by name in its manifest.
-keep class dev.exact.widgets.Native { native <methods>; }
-keep class * extends dev.exact.widgets.Provider
