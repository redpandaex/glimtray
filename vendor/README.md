# Slint text dictionary trimming

GlimTray vendors the published `i-slint-core` 1.18.1 crate so local builds
and GitHub Actions use the same small dependency patch. The UI remains the
current tray-only application; this does not restore the old experimental UI.

Upstream archive: https://static.crates.io/crates/i-slint-core/i-slint-core-1.18.1.crate

Upstream archive SHA256:
`ed2ea15059d70bc31b1a8b1eb4c34f2c5df04cb7e53f36179b43c0261231d646`

The only upstream file changed is `i-slint-core-1.18.1/Cargo.toml`:

```diff
 [target.'cfg(not(target_arch = "wasm32"))'.dependencies.parley]
 version = "0.11.1"
-features = ["complex-scripts"]
 optional = true
```

All Rust sources, original package metadata, copyright notices and upstream
licenses in `i-slint-core-1.18.1/LICENSES/` are retained. `Cargo.toml.orig`
is the unchanged upstream reference; Cargo builds from `Cargo.toml`.

The patch omits ICU dictionary-based segmentation for Chinese/Japanese word
boundaries and Southeast Asian line wrapping. System fonts, glyph shaping,
Unicode text and ordinary character boundaries remain available. GlimTray
currently displays short read-only labels and uses native Windows tray menus.
Reassess this choice before adding editable or long-form multilingual text.

The top-level `slint` and `slint-build` dependencies are pinned to `=1.18.1`.
When upgrading, replace this directory with the verified published crate,
review/reapply the patch, update the path override and pinned versions in
Cargo.toml, then update Cargo.lock. The CI feature check rejects
re-enabling Parley's `complex-scripts` feature. Format/lint the application
package rather than rewriting upstream code.
