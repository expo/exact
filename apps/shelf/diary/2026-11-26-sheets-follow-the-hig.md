surfaces: ios, macos, web
agent: Claude Code, Claude Opus 5.5
task: James: "Make our app follow the HIG and intended native APIs", after comparing Add Book on the closed iPhone Duo with a hand-built UIKit sheet

### Rough
Shelf's sheets used words for buttons the HIG draws as symbols. Add Book and Add Quote had a text "Cancel" and a prominent text "Save" or "Add". The Scan screen, pushed inside Add Book, had a "Cancel" beside UIKit's Back.

The HIG's [Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars) and [Standard icons](https://developer.apple.com/design/human-interface-guidelines/icons) say:
- Close and Cancel use `xmark`; Done and Save use `checkmark`.
- Use the standard Close and Back, not a text label.
- One trailing prominent action.

[Sheets](https://developer.apple.com/design/human-interface-guidelines/sheets) adds:
- Cancel leads and Done trails.
- Don't show Back and Cancel together.

Exact's own patterns already write the xmark named Cancel and the checkmark named Save (LLP 1116.020 D2). The host draws what is written; it does not turn a word into a system item.

On the closed Duo, UIKit puts a sheet's ✕, camera and ✓ in the side strip and keeps the title on top. A hand-built sheet using `.cancel`, an image item and `.done` lands the same way. With Shelf's words it kept "Cancel" and "Save" in the top bar instead.

Save stayed enabled and the form showed red errors after the tap. Apple's sheets, and the patterns since LLP 1116.020 D5, disable Save until the form is valid and say why in a footer.

### Lean in
Each sheet's leading button is `image "symbol:close"` named Cancel, and its confirm is `image "symbol:checkmark"` named Save or Add. Add Book's Save is disabled until the title is set and the pages are valid. The Title footer says "Enter a title to save.", and the Pages footer explains an invalid count as it is typed.

The Scan screen's way out is now the stack's Back control (`id="back"`, `symbol:back`). On iOS that is UIKit's one chevron, and pressing it still runs the scan's cancel, which stops the camera.

On the closed Duo, UIKit now places all three buttons as it places a hand-built sheet's. Unfolded, the sheet shows ✕ at the top leading edge and the camera and ✓ at the trailing edge, as Apple's New Event does. On the Mac, the sheet's Cancel and Save push buttons read the same names.
