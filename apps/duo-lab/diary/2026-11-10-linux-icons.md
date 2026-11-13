# Duo Lab: Linux icon compilation

## Rough

The live Icon Composer artwork required Apple's asset compiler, which does not run on Linux.

## Lean in

Preserve the SVG layers for iOS 26 and newer. Use the manifest's existing PNG as the artwork for older systems. An icon edit must update that PNG too.

## Needed

Portable CoreUI icon stacks and PNG fallbacks, compiled together with the app's other catalog assets.

## Checkpoints

Built on clean Ubuntu 24.04. Apple's CoreUI reader returned the same live light, dark and tinted icon representations as its native compiler, apart from the catalog UUID. The unchanged Linux app passed strict signature verification and a 12-step drive on iOS 27.1: Images, Reflow, and text entry in the native keyboard editor.

The unsigned iPhone release was also built on Linux and delivered through AppDrop. The user opened the editor with its native keyboard on a physical Duo.
