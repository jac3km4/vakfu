## Commands
```bash
# Run linting with warnings treated as errors (required by CI)
cargo clippy --all-targets --all-features -- -D warnings

# Run all tests
cargo test

# Run the application (requires Ankama Games' Wakfu assets)
vakfu --path /path/to/Wakfu --map <map_id>
```

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
src/main.rs       # Application entry point, CLI argument parsing, Bevy App setup
src/assets.rs     # Asset loaders, handling Tgam (textures), Map chunks and elements, sprites
src/render.rs     # MapRenderer resource, responsible for efficiently querying/rendering elements via RTree
src/camera.rs     # Camera controls
src/settings.rs   # Map View Settings and UI logic
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
