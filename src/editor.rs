use std::time::Duration;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::input::ButtonState;
use bevy::input::mouse::MouseButtonInput;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy_egui::egui::{self, Color32, Pos2, Rect as EguiRect, Sense, Vec2 as EguiVec2};
use bevy_egui::{EguiContexts, EguiTextureHandle, PrimaryEguiContext};
use hashbrown::HashMap;
use rstar::RTreeObject;

use crate::assets::{Animation, iso_cell_diamond, iso_to_screen, screen_to_iso};
use crate::camera::{CameraController, MainCamera, OVERLAY_LAYER, is_pointer_over_ui};
use crate::render::{
    ChangeMapEvent, MapCatalog, MapRenderer, Renderable, RenderableSpec, atlas_layout_parts,
    premultiplied_tint,
};
use crate::settings::MapViewSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorTool {
    #[default]
    Select,
    Place,
    Eyedropper,
    Erase,
}

impl EditorTool {
    fn label(self) -> &'static str {
        match self {
            Self::Select => "Select [V]",
            Self::Place => "Place [B]",
            Self::Eyedropper => "Pick [I]",
            Self::Erase => "Erase [E]",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Select => {
                "Click: Select | Drag: Move | [/]: Adjust Z | Del: Delete | Alt+Click: Pick | Mid/Right-Drag: Pan"
            }
            Self::Place => {
                "Click/Drag: Place | [/]: Adjust Z | Alt+Click: Pick | Mid/Right-Drag: Pan"
            }
            Self::Eyedropper => "Click element: Sample sprite, layer & tint | Mid/Right-Drag: Pan",
            Self::Erase => "Click/Drag element: Erase | Mid/Right-Drag: Pan",
        }
    }
}

#[derive(Debug, Clone)]
pub enum EditAction {
    Place(Renderable),
    Remove(Renderable),
    Replace { before: Renderable, after: Renderable },
}

#[derive(Debug, Default)]
pub struct EditorHistory {
    undo_stack: Vec<EditAction>,
    redo_stack: Vec<EditAction>,
}

impl EditorHistory {
    pub fn push(&mut self, action: EditAction) {
        self.undo_stack.push(action);
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, renderer: &mut MapRenderer) -> Option<u64> {
        let action = self.undo_stack.pop()?;
        let focus_uid = match &action {
            EditAction::Place(elem) => {
                renderer.remove_element(elem.uid);
                None
            }
            EditAction::Remove(elem) => {
                let uid = elem.uid;
                renderer.insert_element(elem.clone());
                Some(uid)
            }
            EditAction::Replace { before, .. } => {
                let uid = before.uid;
                renderer.update_element(before.clone());
                Some(uid)
            }
        };
        self.redo_stack.push(action);
        focus_uid
    }

    pub fn redo(&mut self, renderer: &mut MapRenderer) -> Option<u64> {
        let action = self.redo_stack.pop()?;
        let focus_uid = match &action {
            EditAction::Place(elem) => {
                let uid = elem.uid;
                renderer.insert_element(elem.clone());
                Some(uid)
            }
            EditAction::Remove(elem) => {
                renderer.remove_element(elem.uid);
                None
            }
            EditAction::Replace { after, .. } => {
                let uid = after.uid;
                renderer.update_element(after.clone());
                Some(uid)
            }
        };
        self.undo_stack.push(action);
        focus_uid
    }
}

#[derive(Debug, Clone)]
pub struct BrushSettings {
    pub definition_id: Option<i32>,
    pub snap_to_surface: bool,
    pub target_z: i16,
    pub height: u8,
    pub altitude_order: u8,
    pub layer: u8,
    pub group: i32,
    pub color: [f32; 4],
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            definition_id: None,
            snap_to_surface: true,
            target_z: 0,
            height: 0,
            altitude_order: 0,
            layer: 0,
            group: 0,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone)]
struct DragState {
    uid: u64,
    before: Renderable,
    cell_offset: (i32, i32),
}

#[derive(Resource)]
pub struct EditorState {
    pub tool: EditorTool,
    pub selected_uid: Option<u64>,
    drag_state: Option<DragState>,
    painting_active: bool,
    last_painted_cell: Option<(i32, i32)>,
    erasing_active: bool,
    pub brush: BrushSettings,
    pub hovered_surface_z: i16,
    pub show_grid: bool,
    pub hovered_cell: Option<(i32, i32)>,
    pub hovered_world: Option<Vec2>,
    pub palette_page: usize,
    pub palette_only_used: bool,
    sorted_sprite_ids: Vec<i32>,
    pub map_search: String,
    egui_textures: HashMap<i32, (Handle<Image>, egui::TextureId)>,
    ghost_entity: Option<Entity>,
    ghost_def_id: Option<i32>,
    ghost_layouts: HashMap<i32, Handle<TextureAtlasLayout>>,
    pub history: EditorHistory,
    /// Element state captured when an inspector drag/focus edit begins.
    inspector_before: Option<Renderable>,
    last_map_id: Option<i32>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            tool: EditorTool::Select,
            selected_uid: None,
            drag_state: None,
            painting_active: false,
            last_painted_cell: None,
            erasing_active: false,
            brush: BrushSettings::default(),
            hovered_surface_z: 0,
            show_grid: true,
            hovered_cell: None,
            hovered_world: None,
            palette_page: 0,
            palette_only_used: true,
            sorted_sprite_ids: Vec::new(),
            map_search: String::new(),
            egui_textures: HashMap::new(),
            ghost_entity: None,
            ghost_def_id: None,
            ghost_layouts: HashMap::new(),
            history: EditorHistory::default(),
            inspector_before: None,
            last_map_id: None,
        }
    }
}

impl EditorState {
    pub fn effective_brush_z(&self) -> i16 {
        if self.brush.snap_to_surface {
            self.hovered_surface_z.saturating_add(self.brush.target_z)
        } else {
            self.brush.target_z
        }
    }

    fn get_or_register_texture(
        &mut self,
        texture_id: i32,
        assets: &AssetServer,
        contexts: &mut EguiContexts<'_, '_>,
    ) -> egui::TextureId {
        if let Some((_, tex_id)) = self.egui_textures.get(&texture_id) {
            return *tex_id;
        }
        let handle = assets.load::<Image>(format!("gfx://gfx/{texture_id}.tgam"));
        let tex_id = contexts.add_image(EguiTextureHandle::Weak(handle.id()));
        self.egui_textures.insert(texture_id, (handle, tex_id));
        tex_id
    }
}

#[derive(SystemParam)]
pub struct EditorUiParams<'w, 's> {
    contexts: EguiContexts<'w, 's>,
    assets: Res<'w, AssetServer>,
    settings: ResMut<'w, MapViewSettings>,
    catalog: Res<'w, MapCatalog>,
    renderer: ResMut<'w, MapRenderer>,
    camera_controller: ResMut<'w, CameraController>,
    editor: ResMut<'w, EditorState>,
    change_map: MessageWriter<'w, ChangeMapEvent>,
}

pub fn editor_ui_system(params: EditorUiParams<'_, '_>) -> Result {
    let EditorUiParams {
        mut contexts,
        assets,
        mut settings,
        catalog,
        mut renderer,
        mut camera_controller,
        mut editor,
        mut change_map,
    } = params;
    if editor.last_map_id != Some(catalog.current_map_id) {
        editor.last_map_id = Some(catalog.current_map_id);
        editor.selected_uid = None;
        editor.drag_state = None;
        editor.history.clear();
        editor.inspector_before = None;
        let ground_z = renderer.default_ground_z();
        editor.hovered_surface_z = ground_z;
        if !editor.brush.snap_to_surface {
            editor.brush.target_z = ground_z;
        }
        if editor.brush.definition_id.is_none()
            && let Some(&first_def) = renderer.used_definitions().first()
        {
            editor.brush.definition_id = Some(first_def);
        }
    }

    let mut edited_settings = settings.clone();
    let mut undo_clicked = false;
    let mut redo_clicked = false;
    let mut reset_cam_clicked = false;
    let mut duplicate_clicked = false;
    let mut delete_clicked = false;
    let mut bring_front_clicked = false;
    let mut send_back_clicked = false;

    let ctx = contexts.ctx_mut()?.clone();
    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        "editor_viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    // Top Toolbar
    egui::Panel::top("editor_top_bar").show(&mut viewport_ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong("vakfu editor");
            ui.separator();

            for tool in [
                EditorTool::Select,
                EditorTool::Place,
                EditorTool::Eyedropper,
                EditorTool::Erase,
            ] {
                let selected = editor.tool == tool;
                if ui.selectable_label(selected, tool.label()).clicked() {
                    editor.tool = tool;
                }
            }

            ui.separator();
            if ui
                .add_enabled(editor.history.can_undo(), egui::Button::new("↶ Undo"))
                .on_hover_text("Ctrl+Z")
                .clicked()
            {
                undo_clicked = true;
            }
            if ui
                .add_enabled(editor.history.can_redo(), egui::Button::new("↷ Redo"))
                .on_hover_text("Ctrl+Shift+Z / Ctrl+Y")
                .clicked()
            {
                redo_clicked = true;
            }

            ui.separator();
            ui.label("Map:");
            let current_idx = catalog
                .available_maps
                .iter()
                .position(|&id| id == catalog.current_map_id);

            let has_prev = current_idx.is_some_and(|idx| idx > 0);
            if ui.add_enabled(has_prev, egui::Button::new("◀")).clicked()
                && let Some(idx) = current_idx
            {
                change_map.write(ChangeMapEvent(catalog.available_maps[idx - 1]));
            }

            let mut map_search = editor.map_search.clone();
            egui::ComboBox::from_id_salt("editor_map_selector")
                .selected_text(catalog.current_map_id.to_string())
                .width(90.0)
                .show_ui(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("🔍");
                        ui.text_edit_singleline(&mut map_search);
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for &map_id in &catalog.available_maps {
                                let map_str = map_id.to_string();
                                if !map_search.is_empty() && !map_str.contains(&map_search) {
                                    continue;
                                }
                                let is_selected = catalog.current_map_id == map_id;
                                if ui.selectable_label(is_selected, &map_str).clicked() {
                                    change_map.write(ChangeMapEvent(map_id));
                                }
                            }
                        });
                });
            editor.map_search = map_search;

            let has_next = current_idx.is_some_and(|idx| idx + 1 < catalog.available_maps.len());
            if ui.add_enabled(has_next, egui::Button::new("▶")).clicked()
                && let Some(idx) = current_idx
            {
                change_map.write(ChangeMapEvent(catalog.available_maps[idx + 1]));
            }

            ui.separator();
            ui.checkbox(&mut editor.show_grid, "Grid");
            ui.checkbox(&mut edited_settings.enable_light, "Light");
            if ui.button("Center").clicked() {
                reset_cam_clicked = true;
            }
        });
    });

    // Bottom Status Bar
    egui::Panel::bottom("editor_status_bar").show(&mut viewport_ui, |ui| {
        ui.horizontal(|ui| {
            if let Some((cx, cy)) = editor.hovered_cell {
                ui.monospace(format!("Cell: ({cx}, {cy})"));
            } else {
                ui.monospace("Cell: (--, --)");
            }
            ui.separator();
            ui.monospace(format!("Z: {}", editor.effective_brush_z()));
            ui.separator();
            ui.monospace(format!(
                "Zoom: {:.0}%",
                (1.0 / camera_controller.current_zoom) * 100.0
            ));
            ui.separator();
            ui.monospace(format!("Elements: {}", renderer.element_count()));
            ui.separator();
            ui.label(editor.tool.hint());
        });
    });

    // Left Dock: Sprite Palette & Layers
    const PALETTE_TILE_SIZE: f32 = 36.0;
    const PALETTE_TILE_SPACING: f32 = 4.0;
    const PALETTE_TILE_PAD: f32 = 3.0;
    const PALETTE_MIN_COLS: usize = 4;
    const PALETTE_DEFAULT_COLS: usize = 5;

    let palette_grid_width = |cols: usize| -> f32 {
        cols as f32 * PALETTE_TILE_SIZE + cols.saturating_sub(1) as f32 * PALETTE_TILE_SPACING
    };
    let palette_panel_id = egui::Id::new("editor_palette_panel");
    let panel_margin_x = viewport_ui.style().spacing.window_margin.sum().x;
    let scroll_margin_x = viewport_ui.style().spacing.scroll.bar_width
        + viewport_ui.style().spacing.scroll.bar_inner_margin
        + viewport_ui.style().spacing.scroll.bar_outer_margin;
    let min_panel_width =
        palette_grid_width(PALETTE_MIN_COLS) + panel_margin_x + scroll_margin_x;
    let default_panel_width =
        palette_grid_width(PALETTE_DEFAULT_COLS) + panel_margin_x + scroll_margin_x;

    let panel_resp = egui::Panel::left(palette_panel_id)
        .default_size(default_panel_width)
        .min_size(min_panel_width)
        .resizable(true)
        .show(&mut viewport_ui, |ui| {
            ui.heading("Layers");
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(!edited_settings.layer_filter_on, "All")
                    .clicked()
                {
                    edited_settings.layer_filter_on = false;
                }
                for &layer in renderer.available_layers() {
                    let active =
                        edited_settings.layer_filter_on && edited_settings.layer == layer;
                    if ui.selectable_label(active, format!("L{layer}")).clicked() {
                        edited_settings.layer_filter_on = true;
                        edited_settings.layer = layer;
                        editor.brush.layer = layer;
                    }
                }
            });

            ui.separator();
            ui.heading("Sprite Palette");
            if ui
                .checkbox(&mut editor.palette_only_used, "Used in current map")
                .changed()
            {
                editor.palette_page = 0;
            }

            const PALETTE_PAGE_SIZE: usize = 200;
            if !editor.palette_only_used
                && editor.sorted_sprite_ids.len() != catalog.sprites.len()
            {
                let mut ids: Vec<i32> = catalog.sprites.iter().map(|(&id, _)| id).collect();
                ids.sort_unstable();
                editor.sorted_sprite_ids = ids;
            }

            let total_sprites = if editor.palette_only_used {
                renderer.used_definitions().len()
            } else {
                editor.sorted_sprite_ids.len()
            };
            let total_pages = total_sprites.div_ceil(PALETTE_PAGE_SIZE).max(1);
            editor.palette_page = editor.palette_page.min(total_pages - 1);
            let start = editor.palette_page * PALETTE_PAGE_SIZE;
            let end = (start + PALETTE_PAGE_SIZE).min(total_sprites);
            let page_ids: Vec<i32> = if editor.palette_only_used {
                renderer.used_definitions()[start..end].to_vec()
            } else {
                editor.sorted_sprite_ids[start..end].to_vec()
            };

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(editor.palette_page > 0, egui::Button::new("◀").small())
                    .clicked()
                {
                    editor.palette_page -= 1;
                }
                ui.label(
                    egui::RichText::new(format!(
                        "Page {}/{} ({total_sprites})",
                        editor.palette_page + 1,
                        total_pages
                    ))
                    .small()
                    .weak(),
                );
                if ui
                    .add_enabled(
                        editor.palette_page + 1 < total_pages,
                        egui::Button::new("▶").small(),
                    )
                    .clicked()
                {
                    editor.palette_page += 1;
                }
            });

            let mut actual_grid_avail = 0.0_f32;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = EguiVec2::splat(PALETTE_TILE_SPACING);
                    let avail = ui.available_width();
                    actual_grid_avail = avail;
                    let cols = (((avail + PALETTE_TILE_SPACING)
                        / (PALETTE_TILE_SIZE + PALETTE_TILE_SPACING))
                        .floor() as usize)
                        .max(PALETTE_MIN_COLS);

                    for chunk in page_ids.chunks(cols) {
                        ui.horizontal(|ui| {
                            for &def_id in chunk {
                                let Some(def) = catalog.sprites.get(def_id) else {
                                    continue;
                                };
                                let tex_id = editor.get_or_register_texture(
                                    def.texture_id(),
                                    &assets,
                                    &mut contexts,
                                );
                                let is_selected = editor.brush.definition_id == Some(def_id);

                                let (rect, response) = ui.allocate_exact_size(
                                    EguiVec2::splat(PALETTE_TILE_SIZE),
                                    Sense::click(),
                                );

                                let bg = if is_selected {
                                    Color32::from_rgb(55, 95, 145)
                                } else if response.hovered() {
                                    Color32::from_rgb(50, 54, 62)
                                } else {
                                    Color32::from_rgb(34, 37, 42)
                                };
                                ui.painter().rect_filled(rect, 4.0, bg);
                                if is_selected {
                                    ui.painter().rect_stroke(
                                        rect,
                                        4.0,
                                        egui::Stroke::new(1.5, Color32::LIGHT_BLUE),
                                        egui::StrokeKind::Inside,
                                    );
                                }

                                let inner = rect.shrink(PALETTE_TILE_PAD);
                                let (sw, sh) = def.size();
                                let (dw, dh) = def.texture_size();
                                let aspect = if dh > 0 {
                                    f32::from(dw) / f32::from(dh)
                                } else {
                                    1.0
                                };
                                let (img_w, img_h) = if aspect >= 1.0 {
                                    (inner.width(), inner.width() / aspect)
                                } else {
                                    (inner.height() * aspect, inner.height())
                                };
                                let img_rect = EguiRect::from_center_size(
                                    inner.center(),
                                    EguiVec2::new(img_w, img_h),
                                );

                                let pow2 = |v: u16| -> f32 {
                                    let v = u32::from(v.max(1));
                                    (if v < 2 { v } else { v.next_power_of_two() }) as f32
                                };
                                // The atlas canvas follows the padded TGAM
                                // dimensions: the sampled rectangle for static
                                // sprites, the full sheet for animated ones.
                                let (tw, th) = match def.animation() {
                                    Animation::None => (pow2(sw), pow2(sh)),
                                    Animation::Frames(frames) => {
                                        (pow2(frames.full_width()), pow2(frames.full_height()))
                                    }
                                };
                                let uv = match def.animation() {
                                    Animation::None => EguiRect::from_min_max(
                                        Pos2::ZERO,
                                        Pos2::new(f32::from(sw) / tw, f32::from(sh) / th),
                                    ),
                                    Animation::Frames(frames) => {
                                        if let Some(first) = frames.iter().next() {
                                            EguiRect::from_min_max(
                                                Pos2::new(
                                                    f32::from(first.x) / tw,
                                                    f32::from(first.y) / th,
                                                ),
                                                Pos2::new(
                                                    f32::from(first.x + frames.width()) / tw,
                                                    f32::from(first.y + frames.height()) / th,
                                                ),
                                            )
                                        } else {
                                            EguiRect::from_min_max(
                                                Pos2::ZERO,
                                                Pos2::new(f32::from(sw) / tw, f32::from(sh) / th),
                                            )
                                        }
                                    }
                                };

                                ui.painter().image(tex_id, img_rect, uv, Color32::WHITE);

                                if response.clicked() {
                                    editor.brush.definition_id = Some(def_id);
                                    editor.tool = EditorTool::Place;
                                }
                                response.on_hover_text(format!(
                                    "Def #{def_id}\nTexture #{}\n{}x{}",
                                    def.texture_id(),
                                    dw,
                                    dh
                                ));
                            }
                        });
                    }
                });
            actual_grid_avail
        });

    if !viewport_ui.ctx().input(|i| i.pointer.any_down()) {
        let actual_grid_avail = panel_resp.inner;
        let overhead = (panel_resp.response.rect.width() - actual_grid_avail).max(0.0);
        let snapped_cols = (((actual_grid_avail + PALETTE_TILE_SPACING)
            / (PALETTE_TILE_SIZE + PALETTE_TILE_SPACING))
            .round() as usize)
            .max(PALETTE_MIN_COLS);
        let snapped_width = palette_grid_width(snapped_cols) + overhead;
        if (panel_resp.response.rect.width() - snapped_width).abs() > 0.5
            && let Some(mut state) =
                egui::panel::PanelState::load(viewport_ui.ctx(), palette_panel_id)
        {
            state.outer_rect.set_width(snapped_width);
            viewport_ui
                .ctx()
                .data_mut(|d| d.insert_persisted(palette_panel_id, state));
        }
    }

    // Right Dock: Inspector / Brush
    egui::Panel::right("editor_inspector_panel")
        .default_size(250.0)
        .resizable(true)
        .show(&mut viewport_ui, |ui| {
            if let Some(selected_uid) = editor.selected_uid
                && let Some(elem) = renderer.get_element(selected_uid).cloned()
            {
                ui.heading(format!("Element #{}", elem.uid));
                ui.label(format!("Definition: {}", elem.definition_id));
                ui.label(format!("Texture: {}", elem.texture_id));

                if let Some(def) = catalog.sprites.get(elem.definition_id) {
                    let flags = def.flags();
                    ui.horizontal_wrapped(|ui| {
                        if flags.is_walkable() {
                            ui.small("Walkable");
                        }
                        if flags.is_flip() {
                            ui.small("Flipped");
                        }
                        if flags.slope() > 0 {
                            ui.small(format!("Slope:{}", flags.slope()));
                        }
                    });
                }

                ui.separator();
                let mut edited = elem.clone();
                let mut changed = false;
                let mut edit_started = false;
                let mut edit_finished = false;

                egui::Grid::new("element_inspector_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Cell X");
                        let response = ui.add(egui::DragValue::new(&mut edited.cell_x).speed(0.2));
                        changed |= response.changed();
                        edit_started |= response.drag_started() || response.gained_focus();
                        edit_finished |= response.drag_stopped() || response.lost_focus();
                        ui.end_row();

                        ui.label("Cell Y");
                        let response = ui.add(egui::DragValue::new(&mut edited.cell_y).speed(0.2));
                        changed |= response.changed();
                        edit_started |= response.drag_started() || response.gained_focus();
                        edit_finished |= response.drag_stopped() || response.lost_focus();
                        ui.end_row();

                        ui.label("Cell Z");
                        let response = ui.add(egui::DragValue::new(&mut edited.cell_z).speed(0.2));
                        changed |= response.changed();
                        edit_started |= response.drag_started() || response.gained_focus();
                        edit_finished |= response.drag_stopped() || response.lost_focus();
                        ui.end_row();

                        ui.label("Height");
                        let response = ui.add(egui::DragValue::new(&mut edited.height).speed(0.2));
                        changed |= response.changed();
                        edit_started |= response.drag_started() || response.gained_focus();
                        edit_finished |= response.drag_stopped() || response.lost_focus();
                        ui.end_row();

                        ui.label("Alt Order");
                        let response =
                            ui.add(egui::DragValue::new(&mut edited.altitude_order).speed(0.2));
                        changed |= response.changed();
                        edit_started |= response.drag_started() || response.gained_focus();
                        edit_finished |= response.drag_stopped() || response.lost_focus();
                        ui.end_row();

                        ui.label("Layer");
                        let response =
                            ui.add(egui::DragValue::new(&mut edited.layer).range(0..=127));
                        changed |= response.changed();
                        edit_started |= response.drag_started() || response.gained_focus();
                        edit_finished |= response.drag_stopped() || response.lost_focus();
                        ui.end_row();

                        ui.label("Group");
                        let response = ui.add(egui::DragValue::new(&mut edited.group));
                        changed |= response.changed();
                        edit_started |= response.drag_started() || response.gained_focus();
                        edit_finished |= response.drag_stopped() || response.lost_focus();
                        ui.end_row();

                        ui.label("Tint");
                        let mut rgba = [
                            edited.color.red,
                            edited.color.green,
                            edited.color.blue,
                            edited.color.alpha,
                        ];
                        let response = ui.color_edit_button_rgba_unmultiplied(&mut rgba);
                        if response.changed() {
                            edited.color = LinearRgba::new(rgba[0], rgba[1], rgba[2], rgba[3]);
                            edited.color_tinted = edited.color;
                            changed = true;
                        }
                        edit_started |= response.drag_started() || response.gained_focus();
                        edit_finished |= response.drag_stopped() || response.lost_focus();
                        ui.end_row();
                    });

                // Close any previous session before opening a new one, so that
                // moving focus between fields records the finished edit and
                // baselines the field that just gained focus.
                if edit_finished
                    && let Some(before) = editor.inspector_before.take()
                    && let Some(after) = renderer.get_element(before.uid).cloned()
                    && after != before
                {
                    editor.history.push(EditAction::Replace { before, after });
                }
                if edit_started {
                    editor.inspector_before = Some(elem.clone());
                }
                if changed {
                    edited.recompute_position(&mut renderer);
                    renderer.update_element(edited.clone());
                    // Edits that never opened a drag/focus session (such as a
                    // colour popup) are recorded immediately; a tracked session
                    // commits its final state once on release/focus loss.
                    if editor.inspector_before.is_none() {
                        editor.history.push(EditAction::Replace {
                            before: elem,
                            after: edited,
                        });
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Bring Front").clicked() {
                        bring_front_clicked = true;
                    }
                    if ui.button("Send Back").clicked() {
                        send_back_clicked = true;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Duplicate [Ctrl+D]").clicked() {
                        duplicate_clicked = true;
                    }
                    if ui.button("Delete [Del]").clicked() {
                        delete_clicked = true;
                    }
                });
            } else {
                ui.heading("Brush");
                if let Some(def_id) = editor.brush.definition_id {
                    ui.label(format!("Active Sprite: #{def_id}"));
                } else {
                    ui.label("Select a sprite from the Palette");
                }

                ui.separator();
                let prev_snap = editor.brush.snap_to_surface;
                ui.checkbox(&mut editor.brush.snap_to_surface, "Snap Z to surface");
                if prev_snap != editor.brush.snap_to_surface {
                    if editor.brush.snap_to_surface {
                        editor.brush.target_z = 0;
                    } else {
                        editor.brush.target_z = editor.hovered_surface_z;
                    }
                }

                egui::Grid::new("brush_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(if editor.brush.snap_to_surface {
                            "Z Offset"
                        } else {
                            "Target Z"
                        });
                        ui.add(egui::DragValue::new(&mut editor.brush.target_z).speed(0.2));
                        ui.end_row();

                        ui.label("Height");
                        ui.add(egui::DragValue::new(&mut editor.brush.height).speed(0.2));
                        ui.end_row();

                        ui.label("Alt Order");
                        ui.add(
                            egui::DragValue::new(&mut editor.brush.altitude_order).speed(0.2),
                        );
                        ui.end_row();

                        ui.label("Layer");
                        ui.add(egui::DragValue::new(&mut editor.brush.layer).range(0..=127));
                        ui.end_row();

                        ui.label("Group");
                        ui.add(egui::DragValue::new(&mut editor.brush.group));
                        ui.end_row();

                        ui.label("Tint");
                        ui.color_edit_button_rgba_unmultiplied(&mut editor.brush.color);
                        ui.end_row();
                    });
            }
        });

    let avail = viewport_ui.available_rect_before_wrap();
    camera_controller.canvas_rect = Some(Rect::from_corners(
        Vec2::new(avail.min.x, avail.min.y),
        Vec2::new(avail.max.x, avail.max.y),
    ));

    if undo_clicked {
        editor.selected_uid = editor.history.undo(&mut renderer);
    }
    if redo_clicked {
        editor.selected_uid = editor.history.redo(&mut renderer);
    }
    if reset_cam_clicked {
        let center = renderer.center();
        camera_controller.target_position = Vec3::new(center.x, center.y, 0.0);
        camera_controller.target_zoom = 1.0;
    }
    if (bring_front_clicked || send_back_clicked)
        && let Some(uid) = editor.selected_uid
        && let Some(before) = renderer.get_element(uid).cloned()
    {
        let mut after = before.clone();
        after.position.z = if bring_front_clicked {
            renderer.frontmost_z_for(after.envelope(), Some(uid))
        } else {
            renderer.backmost_z_for(after.envelope(), Some(uid))
        };
        renderer.update_element(after.clone());
        editor.history.push(EditAction::Replace { before, after });
    }
    if duplicate_clicked && let Some(uid) = editor.selected_uid {
        duplicate_selected(&mut editor, &mut renderer, uid);
    }
    if delete_clicked && let Some(uid) = editor.selected_uid {
        if let Some(removed) = renderer.remove_element(uid) {
            editor.history.push(EditAction::Remove(removed));
        }
        editor.selected_uid = None;
    }

    settings.set_if_neq(edited_settings);
    Ok(())
}

#[derive(SystemParam)]
pub struct EditorInputParams<'w, 's> {
    time: Res<'w, Time>,
    images: Res<'w, Assets<Image>>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse_buttons: MessageReader<'w, 's, MouseButtonInput>,
    windows: Query<'w, 's, &'static Window>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<MainCamera>>,
    camera_controller: Res<'w, CameraController>,
    egui_contexts: Query<'w, 's, &'static mut bevy_egui::EguiContext, With<PrimaryEguiContext>>,
    catalog: Res<'w, MapCatalog>,
    settings: Res<'w, MapViewSettings>,
    renderer: ResMut<'w, MapRenderer>,
    editor: ResMut<'w, EditorState>,
}

pub fn editor_input_system(params: EditorInputParams<'_, '_>) {
    let EditorInputParams {
        time,
        images,
        keys,
        mut mouse_buttons,
        windows,
        cameras,
        camera_controller,
        mut egui_contexts,
        catalog,
        settings,
        mut renderer,
        mut editor,
    } = params;
    let cursor_pos = windows.single().ok().and_then(Window::cursor_position);
    let canvas_rect = camera_controller.canvas_rect;
    let (is_over_egui, wants_keyboard) =
        egui_contexts
            .iter_mut()
            .fold((false, false), |(over, kbd), mut context| {
                let ctx = context.get_mut();
                (
                    over || is_pointer_over_ui(ctx, canvas_rect, cursor_pos),
                    kbd || ctx.egui_wants_keyboard_input(),
                )
            });

    let elapsed = time.elapsed();

    // Update hovered world & isometric cell (only when pointer is on the map canvas)
    if !is_over_egui
        && let Some(cursor_pos) = cursor_pos
        && let Ok((camera, cam_transform)) = cameras.single()
        && let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor_pos)
    {
        editor.hovered_world = Some(world_pos);
        if editor.brush.snap_to_surface
            && !editor.painting_active
            && let Some(surface) =
                renderer.pick_at_world(world_pos.x, world_pos.y, &settings, &images, elapsed)
        {
            editor.hovered_surface_z = surface.cell_z;
        }
        let eff_z = editor.effective_brush_z();
        editor.hovered_cell = Some(screen_to_iso(world_pos.x, world_pos.y, i32::from(eff_z)));
    } else {
        editor.hovered_world = None;
        editor.hovered_cell = None;
    }

    // Keyboard shortcuts
    let ctrl = keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight)
        || keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let space = keys.pressed(KeyCode::Space);

    if !wants_keyboard {
        if ctrl && keys.just_pressed(KeyCode::KeyZ) {
            if shift {
                editor.selected_uid = editor.history.redo(&mut renderer);
            } else {
                editor.selected_uid = editor.history.undo(&mut renderer);
            }
        } else if ctrl && keys.just_pressed(KeyCode::KeyY) {
            editor.selected_uid = editor.history.redo(&mut renderer);
        } else if ctrl
            && keys.just_pressed(KeyCode::KeyD)
            && let Some(uid) = editor.selected_uid
        {
            duplicate_selected(&mut editor, &mut renderer, uid);
        } else if !ctrl {
            if keys.just_pressed(KeyCode::KeyV) {
                editor.tool = EditorTool::Select;
            } else if keys.just_pressed(KeyCode::KeyB) {
                editor.tool = EditorTool::Place;
            } else if keys.just_pressed(KeyCode::KeyI) {
                editor.tool = EditorTool::Eyedropper;
            } else if keys.just_pressed(KeyCode::KeyE) {
                editor.tool = EditorTool::Erase;
            }

            let z_delta: i16 = if keys.just_pressed(KeyCode::BracketRight) {
                1
            } else if keys.just_pressed(KeyCode::BracketLeft) {
                -1
            } else {
                0
            };
            if z_delta != 0 {
                if editor.tool == EditorTool::Select
                    && let Some(uid) = editor.selected_uid
                    && let Some(before) = renderer.get_element(uid).cloned()
                {
                    let mut after = before.clone();
                    after.cell_z = after.cell_z.saturating_add(z_delta);
                    after.recompute_position(&mut renderer);
                    renderer.update_element(after.clone());
                    editor.history.push(EditAction::Replace { before, after });
                } else {
                    editor.brush.target_z = editor.brush.target_z.saturating_add(z_delta);
                }
            }

            if (keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace))
                && let Some(uid) = editor.selected_uid
            {
                if let Some(removed) = renderer.remove_element(uid) {
                    editor.history.push(EditAction::Remove(removed));
                }
                editor.selected_uid = None;
            }
        }
    }

    // Mouse button handling
    for event in mouse_buttons.read() {
        if event.button != MouseButton::Left {
            continue;
        }
        match event.state {
            ButtonState::Pressed => {
                if is_over_egui || space {
                    continue;
                }
                let Some(world_pos) = editor.hovered_world else {
                    continue;
                };

                if alt || editor.tool == EditorTool::Eyedropper {
                    if let Some(picked) = renderer.pick_at_world(
                        world_pos.x,
                        world_pos.y,
                        &settings,
                        &images,
                        elapsed,
                    ) {
                        editor.brush.definition_id = Some(picked.definition_id);
                        if editor.brush.snap_to_surface {
                            editor.hovered_surface_z = picked.cell_z;
                            editor.brush.target_z = 0;
                        } else {
                            editor.brush.target_z = picked.cell_z;
                        }
                        editor.brush.height = picked.height;
                        editor.brush.altitude_order = picked.altitude_order;
                        editor.brush.layer = picked.layer;
                        editor.brush.group = picked.group;
                        editor.brush.color = [
                            picked.color.red,
                            picked.color.green,
                            picked.color.blue,
                            picked.color.alpha,
                        ];
                        editor.tool = EditorTool::Place;
                    }
                    continue;
                }

                match editor.tool {
                    EditorTool::Select => {
                        if let Some(picked) = renderer
                            .pick_at_world(world_pos.x, world_pos.y, &settings, &images, elapsed)
                            .cloned()
                        {
                            editor.selected_uid = Some(picked.uid);
                            let hover = screen_to_iso(
                                world_pos.x,
                                world_pos.y,
                                i32::from(picked.cell_z),
                            );
                            editor.drag_state = Some(DragState {
                                uid: picked.uid,
                                cell_offset: (picked.cell_x - hover.0, picked.cell_y - hover.1),
                                before: picked,
                            });
                        } else {
                            editor.selected_uid = None;
                        }
                    }
                    EditorTool::Place => {
                        editor.painting_active = true;
                        editor.last_painted_cell = None;
                        if let Some(cell) = editor.hovered_cell {
                            PlacementContext {
                                editor: &mut editor,
                                renderer: &mut renderer,
                                catalog: &catalog,
                                settings: &settings,
                                images: &images,
                                elapsed,
                            }
                            .place_at_cell(cell, Some(world_pos));
                            editor.last_painted_cell = Some(cell);
                        }
                    }
                    EditorTool::Erase => {
                        editor.erasing_active = true;
                        if let Some(picked) = renderer
                            .pick_at_world(world_pos.x, world_pos.y, &settings, &images, elapsed)
                            .cloned()
                        {
                            if editor.selected_uid == Some(picked.uid) {
                                editor.selected_uid = None;
                            }
                            if let Some(removed) = renderer.remove_element(picked.uid) {
                                editor.history.push(EditAction::Remove(removed));
                            }
                        }
                    }
                    EditorTool::Eyedropper => {}
                }
            }
            ButtonState::Released => {
                if let Some(drag) = editor.drag_state.take()
                    && let Some(after) = renderer.get_element(drag.uid).cloned()
                    && (after.cell_x != drag.before.cell_x || after.cell_y != drag.before.cell_y)
                {
                    editor.history.push(EditAction::Replace {
                        before: drag.before,
                        after,
                    });
                }
                editor.painting_active = false;
                editor.last_painted_cell = None;
                editor.erasing_active = false;
            }
        }
    }

    // Continuous drag updates
    if !is_over_egui && !space {
        if let Some(drag) = &editor.drag_state
            && let Some(world_pos) = editor.hovered_world
        {
            let hover = screen_to_iso(world_pos.x, world_pos.y, i32::from(drag.before.cell_z));
            let target_x = hover.0 + drag.cell_offset.0;
            let target_y = hover.1 + drag.cell_offset.1;
            if let Some(mut current) = renderer.get_element(drag.uid).cloned()
                && (current.cell_x != target_x || current.cell_y != target_y)
            {
                current.cell_x = target_x;
                current.cell_y = target_y;
                current.recompute_position(&mut renderer);
                renderer.update_element(current);
            }
        } else if editor.painting_active
            && let Some(cell) = editor.hovered_cell
            && editor.last_painted_cell != Some(cell)
        {
            let wp = editor.hovered_world;
            PlacementContext {
                editor: &mut editor,
                renderer: &mut renderer,
                catalog: &catalog,
                settings: &settings,
                images: &images,
                elapsed,
            }
            .place_at_cell(cell, wp);
            editor.last_painted_cell = Some(cell);
        } else if editor.erasing_active
            && let Some(world_pos) = editor.hovered_world
            && let Some(picked) = renderer
                .pick_at_world(world_pos.x, world_pos.y, &settings, &images, elapsed)
                .cloned()
        {
            if editor.selected_uid == Some(picked.uid) {
                editor.selected_uid = None;
            }
            if let Some(removed) = renderer.remove_element(picked.uid) {
                editor.history.push(EditAction::Remove(removed));
            }
        }
    }
}

/// The editor state and read-only context needed to place elements.
struct PlacementContext<'a> {
    editor: &'a mut EditorState,
    renderer: &'a mut MapRenderer,
    catalog: &'a MapCatalog,
    settings: &'a MapViewSettings,
    images: &'a Assets<Image>,
    elapsed: Duration,
}

impl PlacementContext<'_> {
    fn place_at_cell(&mut self, (cell_x, cell_y): (i32, i32), world_pos: Option<Vec2>) {
        let Some(def_id) = self.editor.brush.definition_id else {
            return;
        };
        let Some(def) = self.catalog.sprites.get(def_id) else {
            return;
        };
        let eff_z = self.editor.effective_brush_z();
        let alt_order = self
            .editor
            .brush
            .altitude_order
            .max(self.renderer.next_altitude_order_at(cell_x, cell_y));

        let (cx, cy) = iso_to_screen(cell_x, cell_y, i32::from(eff_z));
        let mut surface_z_order: Option<f32> = self
            .renderer
            .pick_at_world(cx, cy, self.settings, self.images, self.elapsed)
            .map(|p| p.position.z);
        if let Some(wp) = world_pos
            && let Some(picked) =
                self.renderer
                    .pick_at_world(wp.x, wp.y, self.settings, self.images, self.elapsed)
        {
            surface_z_order =
                Some(surface_z_order.map_or(picked.position.z, |z| z.max(picked.position.z)));
        }

        let [r, g, b, a] = self.editor.brush.color;
        let mut elem = self.renderer.build_renderable(RenderableSpec {
            def,
            cell_x,
            cell_y,
            cell_z: eff_z,
            height: self.editor.brush.height,
            altitude_order: alt_order,
            group: self.editor.brush.group,
            layer: self.editor.brush.layer,
            color: LinearRgba::new(r, g, b, a),
        });
        if let Some(min_front_z) = surface_z_order
            && elem.position.z <= min_front_z
        {
            elem.position.z = self
                .renderer
                .place_above(min_front_z, elem.envelope(), None);
        }
        self.editor.selected_uid = Some(elem.uid);
        self.renderer.insert_element(elem.clone());
        self.editor.history.push(EditAction::Place(elem));
    }
}

fn duplicate_selected(editor: &mut EditorState, renderer: &mut MapRenderer, uid: u64) {
    let Some(mut dup) = renderer.get_element(uid).cloned() else {
        return;
    };
    dup.uid = renderer.allocate_uid();
    dup.cell_x += 1;
    dup.id = None;
    dup.recompute_position(renderer);
    editor.selected_uid = Some(dup.uid);
    renderer.insert_element(dup.clone());
    editor.history.push(EditAction::Place(dup));
}

#[derive(SystemParam)]
pub struct EditorGhostParams<'w, 's> {
    commands: Commands<'w, 's>,
    assets: Res<'w, AssetServer>,
    atlas_layouts: ResMut<'w, Assets<TextureAtlasLayout>>,
    catalog: Res<'w, MapCatalog>,
    windows: Query<'w, 's, &'static Window>,
    camera_controller: Res<'w, CameraController>,
    editor: ResMut<'w, EditorState>,
    egui_contexts: Query<'w, 's, &'static mut bevy_egui::EguiContext, With<PrimaryEguiContext>>,
    ghosts: Query<'w, 's, (&'static mut Sprite, &'static mut Transform)>,
}

pub fn editor_ghost_system(params: EditorGhostParams<'_, '_>) {
    let EditorGhostParams {
        mut commands,
        assets,
        mut atlas_layouts,
        catalog,
        windows,
        camera_controller,
        mut editor,
        mut egui_contexts,
        mut ghosts,
    } = params;
    let cursor_pos = windows.single().ok().and_then(Window::cursor_position);
    let canvas_rect = camera_controller.canvas_rect;
    let is_over_egui = egui_contexts
        .iter_mut()
        .any(|mut context| is_pointer_over_ui(context.get_mut(), canvas_rect, cursor_pos));

    let should_show = editor.tool == EditorTool::Place
        && !is_over_egui
        && editor.hovered_cell.is_some()
        && editor.brush.definition_id.is_some();

    if !should_show {
        if let Some(entity) = editor.ghost_entity.take()
            && commands.get_entity(entity).is_ok()
        {
            commands.entity(entity).despawn();
        }
        editor.ghost_def_id = None;
        return;
    }

    let Some(def_id) = editor.brush.definition_id else {
        return;
    };
    let Some(def) = catalog.sprites.get(def_id) else {
        return;
    };
    let Some((cell_x, cell_y)) = editor.hovered_cell else {
        return;
    };
    let eff_z = editor.effective_brush_z();
    let (sx, sy) = iso_to_screen(
        cell_x,
        cell_y,
        i32::from(eff_z) - i32::from(editor.brush.height),
    );
    let (ox, oy) = def.origin();
    let pos = Vec3::new(sx - ox as f32, sy + oy as f32, 900.0);
    let [r, g, b, a] = editor.brush.color;
    let ghost_color = premultiplied_tint(LinearRgba::new(r, g, b, a * 0.55));

    if editor.ghost_def_id != Some(def_id) {
        if let Some(old) = editor.ghost_entity.take()
            && commands.get_entity(old).is_ok()
        {
            commands.entity(old).despawn();
        }
        let img = assets.load::<Image>(format!("gfx://gfx/{}.tgam", def.texture_id()));
        // Cache one layout per definition: rebuilding it on every definition
        // switch leaked a new TextureAtlasLayout each time.
        let layout = if let Some(layout) = editor.ghost_layouts.get(&def_id) {
            layout.clone()
        } else {
            let (sw, sh) = def.size();
            let layout = atlas_layouts.add(atlas_layout_parts(
                UVec2::new(sw.into(), sh.into()),
                &def.animation(),
            ));
            editor.ghost_layouts.insert(def_id, layout.clone());
            layout
        };
        let (dw, dh) = def.texture_size();
        let mut sprite = Sprite::from_atlas_image(img, layout.into());
        sprite.color = ghost_color;
        sprite.flip_x = def.flags().is_flip();
        sprite.custom_size = Some(Vec2::new(f32::from(dw), f32::from(dh)));
        let entity = commands
            .spawn((
                sprite,
                Anchor::TOP_LEFT,
                Transform::from_translation(pos),
                RenderLayers::layer(OVERLAY_LAYER),
            ))
            .id();
        editor.ghost_entity = Some(entity);
        editor.ghost_def_id = Some(def_id);
    } else if let Some(entity) = editor.ghost_entity
        && let Ok((mut sprite, mut transform)) = ghosts.get_mut(entity)
    {
        if sprite.color != ghost_color {
            sprite.color = ghost_color;
        }
        if transform.translation != pos {
            transform.translation = pos;
        }
    }
}

#[derive(Component)]
pub struct EditorOverlayLine;

struct LineSpec {
    a: Vec2,
    b: Vec2,
    thickness: f32,
    z: f32,
    color: Color,
}

pub fn editor_overlay_system(
    mut commands: Commands<'_, '_>,
    mut pool: Query<
        '_,
        '_,
        (&mut Sprite, &mut Transform, &mut Visibility),
        With<EditorOverlayLine>,
    >,
    editor: Res<'_, EditorState>,
    renderer: Res<'_, MapRenderer>,
    cameras: Query<'_, '_, &Transform, (With<MainCamera>, Without<EditorOverlayLine>)>,
) {
    let zoom = cameras.single().map(|t| t.scale.x).unwrap_or(1.0);
    let thickness = (1.5 * zoom).max(1.0);
    let mut lines = Vec::with_capacity(48);
    let eff_z = i32::from(editor.effective_brush_z());

    // Subtle world-anchored isometric grid when dragging an element
    if editor.show_grid && editor.drag_state.is_some() {
        let z = if editor.brush.snap_to_surface {
            0
        } else {
            i32::from(editor.brush.target_z)
        };
        let center_cell = cameras
            .single()
            .map(|t| screen_to_iso(t.translation.x, t.translation.y, z))
            .unwrap_or((0, 0));
        let radius: i32 = (14.0 * zoom).clamp(10.0, 26.0) as i32;
        let grid_color = Color::srgba(1.0, 1.0, 1.0, 0.03);
        let (cx0, cy0) = iso_to_screen(0, 0, 0);
        let (cx1, cy1) = iso_to_screen(1, 0, 0);
        let dx_off_x = (cx1 - cx0) * 0.5;
        let dy_off_x = (cy1 - cy0) * 0.5;

        for offset in -radius..=radius + 1 {
            let x = center_cell.0 + offset;
            let (ax, ay) = iso_to_screen(x, center_cell.1 - radius, z);
            let (bx, by) = iso_to_screen(x, center_cell.1 + radius + 1, z);
            lines.push(LineSpec {
                a: Vec2::new(ax - dx_off_x, ay - dy_off_x),
                b: Vec2::new(bx - dx_off_x, by - dy_off_x),
                thickness: thickness * 0.75,
                z: 890.0,
                color: grid_color,
            });
        }

        let (cy_x1, cy_y1) = iso_to_screen(0, 1, 0);
        let dx_off_y = (cy_x1 - cx0) * 0.5;
        let dy_off_y = (cy_y1 - cy0) * 0.5;

        for offset in -radius..=radius + 1 {
            let y = center_cell.1 + offset;
            let (ax, ay) = iso_to_screen(center_cell.0 - radius, y, z);
            let (bx, by) = iso_to_screen(center_cell.0 + radius + 1, y, z);
            lines.push(LineSpec {
                a: Vec2::new(ax - dx_off_y, ay - dy_off_y),
                b: Vec2::new(bx - dx_off_y, by - dy_off_y),
                thickness: thickness * 0.75,
                z: 890.0,
                color: grid_color,
            });
        }
    }

    // Hovered isometric cell cursor
    if let Some((cx, cy)) = editor.hovered_cell {
        let d = iso_cell_diamond(cx, cy, eff_z);
        let cursor_color = match editor.tool {
            EditorTool::Select => Color::srgba(0.4, 0.85, 1.0, 0.9),
            EditorTool::Place => Color::srgba(0.35, 1.0, 0.55, 0.95),
            EditorTool::Eyedropper => Color::srgba(1.0, 0.85, 0.3, 0.95),
            EditorTool::Erase => Color::srgba(1.0, 0.35, 0.35, 0.95),
        };
        push_diamond(&mut lines, d, thickness * 1.4, 910.0, cursor_color);
    }

    // Selected element bounding box & base cell diamond
    if let Some(uid) = editor.selected_uid
        && let Some(elem) = renderer.get_element(uid)
    {
        let x = elem.position.x;
        let y = elem.position.y;
        let w = elem.render_size.x as f32;
        let h = elem.render_size.y as f32;
        let highlight = Color::srgba(1.0, 0.85, 0.2, 0.95);
        let box_pts = [
            Vec2::new(x, y),
            Vec2::new(x + w, y),
            Vec2::new(x + w, y - h),
            Vec2::new(x, y - h),
        ];
        for i in 0..4 {
            lines.push(LineSpec {
                a: box_pts[i],
                b: box_pts[(i + 1) % 4],
                thickness: thickness * 1.5,
                z: 920.0,
                color: highlight,
            });
        }

        let base_diamond = iso_cell_diamond(elem.cell_x, elem.cell_y, i32::from(elem.cell_z));
        push_diamond(
            &mut lines,
            base_diamond,
            thickness * 1.3,
            915.0,
            Color::srgba(1.0, 0.6, 0.2, 0.85),
        );
    }

    // Sync pooled line entities without despawning/respawning each frame
    let mut line_iter = lines.into_iter().filter_map(line_geometry);
    for (mut sprite, mut transform, mut vis) in &mut pool {
        if let Some((size, target_transform, color)) = line_iter.next() {
            if sprite.color != color {
                sprite.color = color;
            }
            if sprite.custom_size != Some(size) {
                sprite.custom_size = Some(size);
            }
            transform.set_if_neq(target_transform);
            vis.set_if_neq(Visibility::Inherited);
        } else {
            vis.set_if_neq(Visibility::Hidden);
        }
    }

    for (size, target_transform, color) in line_iter {
        commands.spawn((
            EditorOverlayLine,
            Sprite {
                color,
                custom_size: Some(size),
                ..default()
            },
            target_transform,
            RenderLayers::layer(OVERLAY_LAYER),
        ));
    }
}

fn push_diamond(
    lines: &mut Vec<LineSpec>,
    corners: [(f32, f32); 4],
    thickness: f32,
    z: f32,
    color: Color,
) {
    let pts = corners.map(|(x, y)| Vec2::new(x, y));
    for i in 0..4 {
        lines.push(LineSpec {
            a: pts[i],
            b: pts[(i + 1) % 4],
            thickness,
            z,
            color,
        });
    }
}

fn line_geometry(spec: LineSpec) -> Option<(Vec2, Transform, Color)> {
    let diff = spec.b - spec.a;
    let len = diff.length();
    if len <= 0.001 {
        return None;
    }
    let mid = (spec.a + spec.b) * 0.5;
    let angle = diff.y.atan2(diff.x);
    let mut transform = Transform::from_xyz(mid.x, mid.y, spec.z);
    transform.rotation = Quat::from_rotation_z(angle);
    Some((Vec2::new(len, spec.thickness), transform, spec.color))
}

#[cfg(test)]
mod tests {
    use byte::TryRead;
    use byte::ctx::LittleEndian;

    use super::*;
    use crate::assets::{LightMap, Map, MapSpriteLibrary, iso_to_screen};

    #[test]
    fn screen_to_iso_roundtrips_iso_to_screen() {
        for (x, y, z) in [(0, 0, 0), (5, -3, 2), (-12, 18, -4), (42, 42, 0)] {
            let (wx, wy) = iso_to_screen(x, y, z);
            assert_eq!(screen_to_iso(wx, wy, z), (x, y));
        }
    }

    #[test]
    fn update_and_undo_preserve_entity_and_restore_state() {
        let mut world = World::new();
        let spawned_entity = world.spawn_empty().id();

        let (sprites, _) = MapSpriteLibrary::try_read(&0u32.to_le_bytes(), LittleEndian).unwrap();
        let empty_zip = {
            let mut buf = std::io::Cursor::new(Vec::new());
            zip::ZipWriter::new(&mut buf).finish().unwrap();
            buf.set_position(0);
            buf
        };
        let map = Map::load(empty_zip).unwrap();
        let mut renderer = MapRenderer::new(&map, &sprites, &LightMap::default());
        let mut history = EditorHistory::default();

        let mut initial = Renderable {
            uid: renderer.allocate_uid(),
            definition_id: 10,
            position: Vec3::new(0.0, 0.0, 0.5),
            origin: (0, 0),
            cell_x: 1,
            cell_y: 2,
            cell_z: 0,
            height: 0,
            altitude_order: 0,
            before_mobile: false,
            hashcode: 0,
            texture_size: UVec2::new(32, 32),
            render_size: UVec2::new(32, 32),
            color: LinearRgba::WHITE,
            color_tinted: LinearRgba::WHITE,
            texture_id: 1,
            flip_x: false,
            animation: Animation::None,
            group: 0,
            layer: 0,
            id: None,
        };
        renderer.insert_element(initial.clone());

        // Simulate rendering_system attaching a live Entity to the Renderable
        renderer.attach_entity_for_test(initial.uid, spawned_entity);
        let with_entity = renderer.get_element(initial.uid).unwrap().clone();
        initial.id = Some(spawned_entity);

        // Edit element coordinates and verify the attached Entity is preserved (not despawned)
        let mut moved = with_entity.clone();
        moved.cell_x = 5;
        moved.recompute_position(&mut renderer);
        renderer.update_element(moved.clone());
        history.push(EditAction::Replace {
            before: initial,
            after: moved,
        });

        let updated = renderer.get_element(with_entity.uid).unwrap();
        assert_eq!(updated.cell_x, 5);
        assert_eq!(updated.id, Some(spawned_entity));

        // Undo restores cell_x while keeping the same Entity
        assert_eq!(history.undo(&mut renderer), Some(with_entity.uid));
        assert_eq!(renderer.get_element(with_entity.uid).unwrap().cell_x, 1);
        assert_eq!(
            renderer.get_element(with_entity.uid).unwrap().id,
            Some(spawned_entity)
        );
    }
}
