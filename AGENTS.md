## Commands
```bash
# Build the project
cargo build

# Run linting with warnings treated as errors (required by CI)
cargo clippy -- -D warnings

# Run all tests
cargo test

# Run the application (requires Ankama Games' Wakfu assets)
vakfu --path /path/to/Wakfu --map <map_id>
```

## Boundaries
### Always do
- Run `cargo clippy -- -D warnings` and `cargo test` before submitting a PR.
- Install required system dependencies (`libwayland-dev`, `pkg-config`, `libxkbcommon-dev`, `libasound2-dev`, `libudev-dev`) before building in a fresh environment.

### Ask first
- Any change to the core rendering loop (`src/render.rs`).
- Modifying `Cargo.toml` dependencies.

### Never do
- Explicitly link or refer to the reference files or class names in the documentation when documenting code based on a reference implementation.
- Include any authored game assets in this repository.
- Prefer third-party actions like `dtolnay/rust-toolchain` over `rustup toolchain install stable --profile minimal --component clippy --no-self-update` for GitHub Actions workflows.

## Project Structure
```text
src/main.rs       # Application entry point, CLI argument parsing, Bevy App setup
src/assets.rs     # Asset loaders, handling Tgam (textures), Map chunks and elements, sprites
src/render.rs     # MapRenderer resource, responsible for efficiently querying/rendering elements via RTree
src/camera.rs     # Camera controls
src/settings.rs   # Map View Settings and UI logic
```

## Code Style
```rust
# Preferred: Use standard formatting and descriptive errors
fn parse_map(path: &Path) -> anyhow::Result<Map> {
    let mut file = File::open(path)?;
    // ...
}
```

## Testing
- **Framework:** `cargo test`
- **Focus:** Ensure binary parsing logic in `src/assets.rs` handles valid and invalid formats gracefully.

## Git Workflow
- Branch naming: `feat/[description]`, `fix/[description]`, `chore/[description]`
- PR conventions: Ensure zero clippy warnings before merge.
