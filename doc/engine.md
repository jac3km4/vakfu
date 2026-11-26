# Map Rendering Engine Reference

This document describes the features and data formats of the target map rendering engine, as well as what is missing in the current Rust implementation.

## Features

The map rendering engine is an isometric 2D rendering engine.

### Map Loading

The engine divides the world map into multiple chunks.
Maps are organized in a file based hierarchy.

### Rendering

The engine renders maps composed of isometric elements. Each element belongs to a cell coordinate (X, Y) and altitude (Z).

Features:
- Z-order sorting
- Layer-based rendering and fading
- Group-based rendering and fading
- Highlight fades
- Clipping optimizations
- Colors and color gradients
- Texture masking

### Colors and Lighting
Colors are applied per element.

Colors can be single color or gradients, using RGB or RGBA formats.
Colors can be combined with lighting effects and fade effects (from a hidden element manager or a highlight manager).

### Data Formats

The engine relies on a custom binary format for maps and map elements.

#### Map File Format

The map file stores multiple chunks.

A Map chunk has:
- Coordinates bounds (MinX, MinY, MinZ, MaxX, MaxY, MaxZ)
- Array of groups (Group Keys, Layer Indexes, Group Ids)
- Array of colors
- Sub-chunks defined by rectangles containing multiple elements.

Each element inside a sub-chunk rectangle has:
- `cellZ`: short
- `height`: byte
- `altitudeOrder`: byte
- `occluder`: bit boolean
- Type flags for colors:
    - `0x1`: has tint (RGB)
    - `0x2`: has alpha (A)
    - `0x4`: is gradient
- `elementId`: int
- `groupIndex`: short
- `colorIndex`: short

#### Elements properties (Sprite definitions)

Map elements properties (aka Sprite Definitions) are stored in an elements library.

Each element has:
- `id`: int
- `originX`: short
- `originY`: short
- `imgWidth`: short
- `imgHeight`: short
- `gfxId`: int
- `propertiesFlag`: byte
    - Bits 0-3: Slope (0-15)
    - Bit 4: Flip (boolean)
    - Bit 5: Move top (boolean)
    - Bit 6: Before mobile (boolean)
    - Bit 7: Walkable (boolean)
- `visualHeight`: byte
- `visibilityMask`: byte
- `exportMask`: byte
- `shaderId`: byte
- Animation data (optional)
- `groundSoundType`: byte

#### Day/night colour definitions (`dayLight.xml`)

The client reads a loose `dayLight.xml` from its working directory and falls back
to `contents/data/data.jar!/dayLight.xml`. The file contains one `<season>` per
game-calendar season, each holding `<hour dayPercentage="0..100"
red="0..255" green="0..255" blue="0..255"/>` entries. The sun colour is
interpolated between the two hours surrounding the current day percentage,
wrapping across the end of the season.


## Rust Implementation Comparison

### Implemented Features
- Loading maps, light maps and element definitions from `.jar` files
- Rendering map sprites with reference depth sorting (`LayerOrder` hash codes)
- Applying element colours, including alpha-only and gradient colour tags
- Applying sprite properties (origin offsets, flip, before-mobile ordering, export mask)
- Animated sprites with frame lookup and per-element phase offsets
- Lighting: ambient/shadows/night lights, sun colour interpolated from the
  embedded `dayLight.xml` seasons and hours, automatic day/night intensity from
  sun luminance, and dynamic point lights
- LOD visibility masks (`1` / `3` / `7`)
- Water element animation (`Sea.WATER_ELEMENTS`)
- Indoor/outdoor group visibility from `groups.lib` with 300 ms fades
- Picking using the TGAM alpha mask, with an RGBA-alpha fallback
- Premultiplied-alpha blending and the reference `gColorScale` tint

### Missing Features

1. **Occluders and Clipping**: The target engine cuts holes around occluded entities and clips sprites; this is not implemented.
2. **Hidden Element / HighLight Fades**: Per-element highlight fades and hover fades are not implemented (group/layer fades are).
3. **Slope / Height Processing**: Highlight transforms based on the slope mask and visual height.
4. **Ground Sound Type**: Extracted during parsing but unused.
5. **Shaders**: The per-element shader id is ignored.
6. **Render Tree and Stencils**: Multi-cell entity sorting, stencil mask overlays, and the specialised render tree.
7. **Scripted Lighting**: Scripted `LitSceneModifier`s and ambiance-driven
   lighting overrides (the reference `MapLightingModifierModel` effect blocks)
   are not applied, and the day percentage and season are picked manually rather
   than following the game calendar.
8. **Dynamic Entities, Particles and Ambiance**: `.anm` elements, particle systems, weather/cloud effects, parallax and global colour grading.
9. **Async Chunk Streaming**: The viewer loads a whole map jar per map rather than streaming chunks around the camera.
