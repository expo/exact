// A List's rows on the JS target: each kept in a semantic list item (navigation.js `listItem`), whose role follows the
// row's own and its owner's. Imported by an app's module only when its plan has a List (exact-web-js `commands.rs`,
// LLP 1047.002 D1); tree.js reaches it through `Hooks.listItem` and `Hooks.listRoles`.
import { listItem, syncListItemRoles } from './navigation.js';
import { Hooks } from './rt.js';

Hooks.listItem = listItem;
Hooks.listRoles = syncListItemRoles;
