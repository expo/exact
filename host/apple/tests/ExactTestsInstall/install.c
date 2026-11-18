// The test bundle's composition, installed as the bundle loads and before
// any test, as an app installs its own before its first session (LLP
// 1047.001 D4): `exact_tests_install` is `TestsInstall.swift`'s. A family
// whose Swift is in its own target is reached only through the slots its
// installer sets.
#include "ExactTestsInstall.h"

// Read by `TestsInstall.swift`, which keeps this object in the bundle: the
// linker drops an archive member nothing references, constructor and all.
int exact_tests_installed = 0;

__attribute__((constructor)) static void install(void) { exact_tests_install(); }
