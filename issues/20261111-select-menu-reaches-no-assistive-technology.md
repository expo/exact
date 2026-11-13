# A select's menu on the painted hosts reaches no assistive technology, and is no platform popup on Android or Windows

**Status:** Open
**Progress:** The accessibility part landed: a select is `EXPANDABLE`, and while
its menu is open the presenter's tree is the select, `EXPANDED`, over a listbox
of the options that show (`presenter/access/menu.rs`), the chosen one selected;
a press on an option chooses it and the listbox scrolls. Windows maps them to UI
Automation's ComboBox (ExpandCollapse), List and ListItem (SelectionItem), with
its focus on the row the keys are at; Android to a Spinner's ListView and
CheckedTextView. The presentation part remains: on Android and Windows a
select's menu is still painted by the presenter, not opened as the top layer's
menu.
**Systems:** host/painted/src/presenter/access.rs, host/painted/src/presenter/select.rs, host/painted/src/presenter/control.rs
**Severity:** P2
**Author:** Claude (Opus 5.5)
**Date:** 2026-11-11

## What is wrong

What is left of `issues/closed/20261110-select-menu-and-keys-on-painted-hosts.md`
after its keys, Escape, Tab, Back and the long menu's bound and scroll were built
(`presenter/select.rs`). The presenter in `host/painted/src/presenter/` is the one
Linux, Windows and Android link.

- **Accessibility.** A select is one leaf node (`presenter/access.rs`,
  `access_control`, `ControlKind::Select`): `access_tree` never reads `self.menu`,
  so an open menu's options are no nodes, the page under it is still told, and the
  dropdown has neither `EXPANDABLE` nor `EXPANDED`. `access_act` has no action that
  sets a dropdown's value.
- **Presentation.** The menu is painted by the presenter over the frame on every
  painted host. Android and Windows have the top layer's menu (`presenter/top.rs`,
  `presenter/top/look/menu.rs`), which their shells show as a platform popup and
  whose rows `presenter/access/surface.rs` tells; a select's menu does not use it.

## Expected

HTML-AAM maps a `select` to a combobox whose popup holds its options: an open menu
tells its options as nodes, the chosen one selected, leaves the page under it out,
and marks the dropdown expanded. Android's `Spinner` and Material's exposed dropdown
menu and WinUI's `ComboBox` each show the list as a popup of the platform's and
expose its items.

## Direction

On Android and Windows open a select's menu as the top layer's menu; on desktop
Linux, which presents no menu popover, tell the painted menu's rows from
`access_tree` while `self.menu` is set.

## Related

LLP 1069.001 D7 designs LLP 1021's light-dismiss popover for a select; its as-built
for slice 2 says the menu's rows are not nodes. LLP 1115.001's as-built lists "a
`select`'s menu" as short of its D10 on Android, and LLP 1115.004 D10.4 plans a
Fluent `ComboBox` list for Windows.
