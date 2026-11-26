## Commands
```bash
# Run linting with warnings treated as errors (required by CI)
cargo clippy --all-targets --all-features -- -D warnings

# Run all tests
cargo test

# Run the viewer (requires Ankama Games' Wakfu assets)
vakfu --path /path/to/Wakfu

# Run the interactive map editor (same arguments)
editor --path /path/to/Wakfu
```

Both binaries pick the map from the in-app selector.

## Boundaries
### Always do
- Run `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test` before submitting a PR.
- Install required system dependencies (`libwayland-dev`, `pkg-config`, `libxkbcommon-dev`, `libasound2-dev`, `libudev-dev`) before building in a fresh environment.

### Ask first
- Modifying `Cargo.toml` dependencies.

### Never do
- Prefer third-party actions like `dtolnay/rust-toolchain` over just invoking builtin commands like `rustup toolchain install stable --profile minimal --component clippy --no-self-update` for GitHub Actions workflows.

## Project Structure
```text
src/lib.rs            # Library root, re-exporting the shared modules below
src/app.rs            # Shared CLI parsing, archive discovery, LoadedWorld and VakfuCorePlugin
src/main.rs           # Map viewer binary
src/bin/editor.rs     # Map editor binary
src/editor.rs         # Editor tools, undo/redo history, egui panels and overlay systems
src/assets.rs         # JarAssetSource pool, TgamLoader and the TGAM mask library
src/assets/groups.rs  # Indoor/outdoor group visibility parsed from groups.lib
src/assets/light.rs   # Per-cell light maps, time-of-day LightingState and point lights
src/assets/map.rs     # Map chunks, element records, colour tags and isometric projection
src/assets/sprite.rs  # Sprite definitions (elements.lib), flags and animation frames
src/assets/tgam.rs    # TGAM texture and packed alpha-mask parser
src/render.rs         # MapRenderer R-tree index, Z ordering, culling, picking and animation
src/premult_alpha.rs  # Premultiplied-alpha pipeline remapping for Bevy 0.20 sprites
src/camera.rs         # Smooth pan/zoom, camera layers and shared egui input capture
src/settings.rs       # Viewer settings UI plus shared map selector and lighting controls
src/util.rs           # Byte parsing helpers (WithSize, WithSizePrefix)
```

## Testing
- **Framework:** `cargo test`

## Git Workflow
Branch naming:
  feat/[short-description]
  fix/[short-description]
  chore/[short-description]

Commit format: [prefix]: [what changed in imperative mood]
  Example: feat: add DWARF v5 support for symbols

PR name format: [prefix]: [what changed in imperative mood]
  Example: feat: add DWARF v5 support for symbols
