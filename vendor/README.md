# Slint runtime trimming

GlimTray vendors the published `i-slint-core` 1.18.1 crate so local builds
and GitHub Actions use the same dependency patches for the Windows tray UI.

Upstream archive: https://static.crates.io/crates/i-slint-core/i-slint-core-1.18.1.crate

Upstream archive SHA256:
`ed2ea15059d70bc31b1a8b1eb4c34f2c5df04cb7e53f36179b43c0261231d646`

## Local patches

Only two upstream files are changed:

- `i-slint-core-1.18.1/Cargo.toml`:
  - Remove Parley's desktop `complex-scripts` feature to omit dictionary data.
  - Enable only PNG decoding in the normal `image` dependency.
  - Keep the `svg` feature and `resvg` vector renderer, without
    `i-slint-common/svg-text` or resvg's `text`, `raster-images` and `svgz` features.
- `i-slint-core-1.18.1/graphics/image/svg.rs`:
  - Use default SVG options instead of the shared SVG font-resolution bridge.

All other upstream sources, original package metadata, copyright notices and
licenses in `i-slint-core-1.18.1/LICENSES/` are retained unchanged.
`Cargo.toml.orig` is the unchanged upstream reference; Cargo builds from
`Cargo.toml`.

## Retained and omitted support

Ordinary Slint UI text, system fonts, glyph shaping, Unicode character handling,
PNG images and SVG vector paths remain available. SVG text is separate from
ordinary UI text; removing the SVG font bridge does not remove monitor names,
percentages or error labels.

The patches omit:

- ICU dictionary-based Chinese/Japanese word boundaries and Southeast Asian
  dictionary-based line wrapping.
- Runtime JPEG decoding.
- SVG text, embedded raster images and compressed SVGZ input.

The current UI embeds three PNG images and four plain vector SVGs from the
standard widgets. `scripts/check-runtime-assets.ps1` inspects generated Rust
resources after a Release build and rejects unsupported image formats or SVG
text/raster elements. It checks static data without running the application.
Reassess the patches before adding new image formats or editable/long-form
multilingual text.

## Maintenance

The top-level `slint` and `slint-build` dependencies are pinned to `=1.18.1`.
When upgrading, replace this directory with the verified published crate,
review/reapply both patches, update the path override and pinned versions in
Cargo.toml, then update Cargo.lock.

CI checks the target runtime feature tree with
`--edges normal,features,no-proc-macro`; the compiler and procedural macros may
still use full image features on the build host. CI also checks embedded assets
before packaging. Format/lint the application package rather than rewriting
upstream code.
