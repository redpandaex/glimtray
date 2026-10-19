# Slint runtime trimming

GlimTray vendors three published Slint 1.18.1 crates so local builds and
GitHub Actions use the same dependency patches for the Windows tray UI:
`i-slint-core`, `i-slint-backend-winit` and `i-slint-renderer-femtovg`.

Upstream archives and SHA256 hashes:

| Crate | Upstream archive | SHA256 |
| --- | --- | --- |
| `i-slint-core` | https://static.crates.io/crates/i-slint-core/i-slint-core-1.18.1.crate | `ed2ea15059d70bc31b1a8b1eb4c34f2c5df04cb7e53f36179b43c0261231d646` |
| `i-slint-backend-winit` | https://static.crates.io/crates/i-slint-backend-winit/i-slint-backend-winit-1.18.1.crate | `dae4fcf1b19e9cb109984d2af87d2b9485388c619265bff71dd523133888bee2` |
| `i-slint-renderer-femtovg` | https://static.crates.io/crates/i-slint-renderer-femtovg/i-slint-renderer-femtovg-1.18.1.crate | `eb0de504d57cdadf80c2abf7bcb8b7135a3716aee1f41b6ebb90ea93e3649d7c` |

## Local patches

- `i-slint-core-1.18.1/Cargo.toml`:
  - Remove Parley's desktop `complex-scripts` feature to omit dictionary data.
  - Enable only PNG decoding in the normal `image` dependency.
  - Keep the `svg` feature definition, but the vendored backend and renderer no
    longer enable it. The remaining definition also omits
    `i-slint-common/svg-text` and resvg's `text`, `raster-images` and `svgz` features.
- `i-slint-core-1.18.1/graphics/image/svg.rs`:
  - Use default SVG options instead of the shared SVG font-resolution bridge.
- `i-slint-backend-winit-1.18.1/Cargo.toml` and
  `i-slint-renderer-femtovg-1.18.1/Cargo.toml`:
  - Remove `svg` from the non-wasm `i-slint-core` dependency features. The UI
    only uses PNG assets, so runtime SVG decoding and its font bridge are not
    linked into the executable.

All other upstream sources, original package metadata, copyright notices and
licenses in each vendored `LICENSES/` directory are retained unchanged.
`Cargo.toml.orig` is the unchanged upstream reference; Cargo builds from
`Cargo.toml`.

## Retained and omitted support

Ordinary Slint UI text, system fonts, glyph shaping, Unicode character handling
and PNG images remain available. The application includes a small custom slider
instead of the standard widget set, so no SVG assets are generated for the UI.

The patches omit:

- ICU dictionary-based Chinese/Japanese word boundaries and Southeast Asian
  dictionary-based line wrapping.
- Runtime JPEG decoding.
- Runtime SVG decoding, including SVG text, embedded raster images and
  compressed SVGZ input.

The current UI embeds two PNG images and no vector SVGs.
`scripts/check-runtime-assets.ps1` inspects generated Rust resources after a
Release build and rejects unsupported image formats or SVG text/raster elements.
It checks static data without running the application. Reassess the patches
before adding new image formats, SVG assets or editable/long-form multilingual
text.

## Maintenance

The top-level `slint` and `slint-build` dependencies are pinned to `=1.18.1`.
When upgrading, replace all three vendored directories with the verified
published crates, review and reapply the patches above, update the path
overrides and pinned versions in Cargo.toml, then update Cargo.lock and the
hashes in this file.

CI checks the target runtime feature tree with
`--edges normal,features,no-proc-macro`; the compiler and procedural macros may
still use full image and SVG features on the build host. CI also checks embedded
assets before packaging. Format/lint the application package rather than
rewriting upstream code.
