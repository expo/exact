# Editor Support for Exact Contract and Forma

- **VS Code / Cursor / Windsurf / VSCodium**: see [vscode/README.md](vscode/README.md).
  Provides TextMate grammars for `.contract` and `.forma`, snippets, `$exact-contract`
  problem matcher, and opt-in hooks for TypeScript 7 content mappers
  ([content-mapper](content-mapper)) and `contract fmt` / warnings.
- **Neovim**: copy or source [neovim/exact-contract.lua](neovim/exact-contract.lua)
  for `.contract` and `.forma` filetypes, comment strings, and 2-space indentation.
- **Helix**: merge [helix/languages.toml](helix/languages.toml) into
  `~/.config/helix/languages.toml`.
- **Sublime Text / Shiki / Bat / GitHub Linguist**: reuse
  [vscode/syntaxes/exact-contract.tmLanguage.json](vscode/syntaxes/exact-contract.tmLanguage.json)
  and [vscode/syntaxes/exact-forma.tmLanguage.json](vscode/syntaxes/exact-forma.tmLanguage.json)
  directly.
