/* `TestsInstall.swift`'s installer, which install.c's constructor calls as the
 * test bundle loads, and the flag it sets once it has run. */
void exact_tests_install(void);
extern int exact_tests_installed;
