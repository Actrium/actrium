# Actro application icon

`actro-mark.svg` is the canonical vector source for the Actro application
icon. Do not edit generated PNG or ICO files by hand.

Regenerate every platform's icon resources from the repository root:

```sh
npm run generate:app-icons
```

The generator writes:

- iOS light, dark and tinted 1024 px icons, plus mac-compatible slots used by
  the multiplatform iOS target;
- the complete macOS `AppIcon.appiconset`;
- Android legacy, round, adaptive foreground and monochrome launcher icons at
  every density, plus the Play Store image;
- a multi-resolution Windows ICO and common PNG sizes.

The mark preserves its Möbius-like geometry as two continuous flat-color bands:
gold `#FFB800` and cobalt blue `#246BFD`. Path order alone communicates the
over/under weave. Generated icons deliberately avoid gradients, highlights, and
shadows so the silhouette and negative space remain legible at small sizes.
Platform assets currently present the mark with a 30-degree clockwise rotation;
override this with `--rotation` when running the generator for visual experiments.
