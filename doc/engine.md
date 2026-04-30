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


## Rust Implementation Comparison

The current Rust implementation provides basic map loading and rendering but lacks several features of the target engine.

### Implemented Features
- Loading maps from `.jar` files
- Loading element definitions (`elements.lib` inside `data.jar`)
- Rendering map sprites with basic depth sorting
- Applying element colors
- Applying basic sprite properties (origin offsets, flip)
- Simple animations

### Missing Features

1. **Occluders and Clipping**: The target engine has an optimization using occluders and clipping flags. This is completely missing in Rust.
2. **Hidden Element / HighLight Fades**: The target engine supports applying colors based on highlight fades and layer fades.
3. **Lighting**: The target engine supports applying specific lighting colors per sprite, substituting material properties dynamically.
4. **Hit Testing / Masking**: The target engine provides precise hit testing, evaluating exact pixel locations against alpha masks. Texture mask parsing in Rust currently extracts the mask, but it is not used in rendering or interactions.
5. **Slope / Height Processing**: The target engine computes specific transforms when highlighting based on the slope mask and visual height.
6. **Ground Sound Type**: Extracted in the target engine, missing in Rust.
7. **Shaders**: The target engine can specify a shader ID per element. The Rust version ignores this flag.
8. **Visibility Masks**: The target engine supports elements with visibility masks.
9. **Render Tree and Stencils**: The target engine uses a specialized rendering tree structure for rendering ordering, multi-cell elements, and handling mask overlays using Stencil buffers, missing in Rust.
10. **Camera Masking**: The target engine supports tracking elements and applying camera mask keys and group limits based on the camera view.
11. **Picking**: The target engine implements picking using hit testing for elements selection which is unimplemented in the Rust version.
12. **Daylight and Scripted Lighting**: The target engine features managers for day percentage color gradients and scripted modifiers to programmatically mutate scene colors in real-time.
