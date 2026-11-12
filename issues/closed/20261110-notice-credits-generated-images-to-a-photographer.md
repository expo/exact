# NOTICE says the gallery photographs are by a named person; their provenance files and content credentials say they were generated

**Status:** Closed
**Resolution:** NOTICE no longer credits the gallery pictures to Daehyeon Mun and says, as their provenance files and C2PA manifests do, that they were generated, naming all four directories that hold them; `apps/duo-lab/art/` stays in the credited list, since nothing in the repository says who made its icon.
**Systems:** NOTICE, apps/exact-live/assets, apps/interaction-gallery/assets, apps/duo-lab/assets, apps/shared-elements/assets
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

Three records in the repository at `dd6f5dcd` describe the same six pictures, and they disagree. This issue reports what each says; which is right is for whoever holds the permission to settle.

**NOTICE**, lines 10-15:

```
The following images are by Daehyeon Mun and are included with permission:
  examples/calendar/assets/themes/
  apps/exact-live/ (photographs)
  apps/interaction-gallery/ (photographs)
  apps/duo-lab/art/
  tamber/tests/import/figma/ (Figma kit fixtures)
```

The only image files under `apps/exact-live/` and `apps/interaction-gallery/` are six PNGs in each `assets/` directory: `north-shore`, `ochre-dunes`, `alpine-water`, `winter-ridge`, `glasshouse`, `last-light`. The two sets are byte-identical.

**The provenance files** beside them, `apps/exact-live/assets/provenance.json:2-4` and `apps/interaction-gallery/assets/provenance.json:2-4`:

```json
"generator": "OpenAI built-in image_gen tool",
"created": "2026-09-17",
"kind": "Generated photographic illustrations of imagined locations; no real location, photographer or stock license claimed.",
```

Each lists the six files with a SHA-256 that matches the committed file.

**The images' embedded content credentials.** Each of the twelve PNGs has a `caBX` chunk holding a C2PA manifest. Its text fields read: action `c2pa.created`, `when` `2026-09-17T00:00:00Z`, `softwareAgent` name `gpt-image`, `digitalSourceType` `http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia`, claim generator `OpenAI Media Service API`, and a certificate naming `OpenAI OpCo, LLC`. The signature was not validated.

Two more directories hold the same pictures and are not in NOTICE's list:

- `apps/duo-lab/assets/` has four of the PNGs, byte-identical, with the same embedded manifest and its own copy of the provenance file (line 50: "From apps/interaction-gallery/assets on 2026-10-02 … the same bytes, the same provenance."). NOTICE names `apps/duo-lab/art/`, which holds two SVGs and `icon.json`.
- `apps/shared-elements/assets/` has six JPEGs. Its `provenance.json:2` gives their source as "apps/interaction-gallery/assets (see its provenance.json): generated photographic illustrations of imagined locations, no license claimed". The JPEGs carry no manifest.

## Verification

Read NOTICE and the four `provenance.json` files. Computed SHA-256 for every PNG in the three `assets/` directories and compared them with each other and with the provenance files. Read each PNG's chunk list and the printable strings of its `caBX` chunk (a metadata read; no signature check). The PNGs under `examples/calendar/assets/themes/` and `apps/duo-lab/assets/icon.png` have no `caBX` chunk. Not checked: the origin of anything else NOTICE lists.

## Direction

Have NOTICE and the provenance files say the same thing about these six pictures, and have NOTICE's list cover every directory that holds them or none. The person who arranged the permission should confirm which statement stands, and whether `apps/duo-lab/art/` was the directory meant.

## Related

No issue or QUEUE entry mentions NOTICE or these provenance files.
