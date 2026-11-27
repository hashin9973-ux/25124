use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use bevy::{
    app::AppExit,
    input_focus::{
        FocusCause, InputFocus, InputFocusVisible,
        tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin},
    },
    input::mouse::{MouseScrollUnit, MouseWheel},
    picking::hover::{HoverMap, Hovered},
    prelude::*,
    text::{FontSize, TextFont},
    ui::Pressed,
    ui_widgets::{Activate, Button, observe},
};
use serde::Deserialize;

use crate::{
    config::{RuntimeConfig, WorldLibrary},
    engine::AppMode,
    persistence::{WorldSaveTarget, WorldStateFile},
    physics::DestructionJournal,
};
use avian3d::prelude::Gravity;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TabNavigationPlugin)
            .init_resource::<MenuState>()
            .init_resource::<MenuFeedback>()
            .init_resource::<QuitConfirmationState>()
            .add_systems(Startup, setup_menu)
            .add_systems(
                Update,
                (
                    toggle_menu_with_escape,
                    update_button_styles,
                    sync_menu_visibility,
                    update_world_list_scroll,
                    sync_menu_feedback,
                    sync_quit_confirmation_visibility,
                ),
            );
    }
}

#[derive(Resource, Default)]
pub(crate) struct MenuState {
    pub(crate) visible: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MenuFeedbackTone {
    #[default]
    Notice,
    Success,
}

#[derive(Resource, Default)]
pub(crate) struct MenuFeedback {
    pub(crate) message: String,
    pub(crate) tone: MenuFeedbackTone,
}

#[derive(Resource, Default)]
struct QuitConfirmationState {
    visible: bool,
    return_focus: Option<Entity>,
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct WorldList;

#[derive(Component)]
struct MenuNotice;

#[derive(Component, Clone, Copy)]
struct MenuNoticeColors {
    notice: Color,
    success: Color,
}

#[derive(Component)]
struct QuitConfirmationPanel;

#[derive(Component)]
struct MenuAction(String);

#[derive(Component)]
struct LibraryOnly;

#[derive(Component)]
struct SimulationOnly;

#[derive(Component, Clone, Copy)]
struct MenuButtonColors {
    normal: Color,
    hovered: Color,
    border_normal: Color,
    border_active: Color,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MenuAsset {
    schema_version: u32,
    title: String,
    subtitle: String,
    #[serde(default = "default_empty_library_title")]
    empty_library_title: String,
    #[serde(default = "default_empty_library_hint")]
    empty_library_hint: String,
    #[serde(default = "default_quit_confirm_title")]
    quit_confirm_title: String,
    #[serde(default = "default_quit_confirm_message")]
    quit_confirm_message: String,
    #[serde(default = "default_quit_confirm_label")]
    quit_confirm_label: String,
    #[serde(default = "default_quit_cancel_label")]
    quit_cancel_label: String,
    #[serde(default = "default_library_navigation_hint")]
    library_navigation_hint: String,
    #[serde(default = "default_pause_controls_hint")]
    pause_controls_hint: String,
    #[serde(default = "default_pause_title")]
    pause_title: String,
    #[serde(default = "default_world_count_template")]
    world_count_template: String,
    #[serde(default = "default_library_warning_template")]
    library_warning_template: String,
    #[serde(default = "default_modal_backdrop_srgba")]
    modal_backdrop_srgba: [f32; 4],
    #[serde(default = "default_notice_srgba")]
    notice_srgba: [f32; 4],
    #[serde(default = "default_success_srgba")]
    success_srgba: [f32; 4],
    #[serde(default = "default_library_warning_srgba")]
    library_warning_srgba: [f32; 4],
    background_srgba: [f32; 4],
    panel_srgba: [f32; 4],
    title_srgba: [f32; 4],
    subtitle_srgba: [f32; 4],
    button_srgba: [f32; 4],
    button_hover_srgba: [f32; 4],
    button_text_srgba: [f32; 4],
    #[serde(default = "default_panel_border_srgba")]
    panel_border_srgba: [f32; 4],
    #[serde(default = "default_panel_shadow_srgba")]
    panel_shadow_srgba: [f32; 4],
    #[serde(default = "default_panel_shadow_blur_px")]
    panel_shadow_blur_px: f32,
    #[serde(default = "default_panel_shadow_offset_y_px")]
    panel_shadow_offset_y_px: f32,
    #[serde(default = "default_button_border_srgba")]
    button_border_srgba: [f32; 4],
    #[serde(default = "default_button_active_border_srgba")]
    button_active_border_srgba: [f32; 4],
    #[serde(default = "default_danger_button_srgba")]
    danger_button_srgba: [f32; 4],
    #[serde(default = "default_danger_button_hover_srgba")]
    danger_button_hover_srgba: [f32; 4],
    panel_width_px: f32,
    panel_padding_px: f32,
    vertical_gap_px: f32,
    title_size_px: f32,
    subtitle_size_px: f32,
    button_width_px: f32,
    button_height_px: f32,
    button_font_size_px: f32,
    #[serde(default = "default_world_card_height_px")]
    world_card_height_px: f32,
    #[serde(default = "default_world_card_gap_px")]
    world_card_gap_px: f32,
    #[serde(default = "default_world_preview_size_px")]
    world_preview_size_px: f32,
    #[serde(default = "default_world_description_size_px")]
    world_description_size_px: f32,
    #[serde(default = "default_world_metadata_size_px")]
    world_metadata_size_px: f32,
    #[serde(default = "default_world_empty_description")]
    world_empty_description_text: String,
    #[serde(default = "default_world_preview_label")]
    world_preview_fallback_label: String,
    #[serde(default = "default_world_version_prefix")]
    world_preview_version_prefix: String,
    #[serde(default = "default_world_format_label")]
    world_format_label: String,
    #[serde(default = "default_world_regions_label")]
    world_regions_label: String,
    #[serde(default = "default_world_objects_label")]
    world_objects_label: String,
    #[serde(default = "default_world_metadata_max_fields")]
    world_metadata_max_fields: usize,
    #[serde(default = "default_panel_radius_px")]
    panel_radius_px: f32,
    #[serde(default = "default_card_radius_px")]
    card_radius_px: f32,
    buttons: Vec<MenuButtonDefinition>,
}

fn default_empty_library_title() -> String {
    "YOUR WORLD LIBRARY IS EMPTY".to_owned()
}

fn default_empty_library_hint() -> String {
    "Add a world manifest at worlds/<name>/world.json to get started.".to_owned()
}

fn default_quit_confirm_title() -> String {
    "UNSAVED SIMULATION".to_owned()
}

fn default_quit_confirm_message() -> String {
    "Changes since your last F5 save will be lost. Choose Keep Playing to resume and save before quitting."
        .to_owned()
}

fn default_quit_confirm_label() -> String {
    "QUIT ANYWAY".to_owned()
}

fn default_quit_cancel_label() -> String {
    "KEEP PLAYING".to_owned()
}

fn default_library_navigation_hint() -> String {
    "TAB / SHIFT+TAB NAVIGATE   ·   ENTER SELECT".to_owned()
}

fn default_pause_controls_hint() -> String {
    "HOLD RMB TO LOOK  ·  WASD MOVE  ·  SHIFT SPRINT  ·  SPACE/CTRL VERTICAL\nTAB NAVIGATE  ·  ENTER ACTIVATE  ·  ESC RESUME  ·  F5 SAVE".to_owned()
}

fn default_pause_title() -> String {
    "SIMULATION PAUSED".to_owned()
}

fn default_world_count_template() -> String {
    "{count} WORLDS AVAILABLE".to_owned()
}

fn default_library_warning_template() -> String {
    "{count} LIBRARY NOTICE(S) — Some entries or previews were skipped. See the application log."
        .to_owned()
}

fn default_modal_backdrop_srgba() -> [f32; 4] {
    [0.004, 0.008, 0.016, 0.88]
}

fn default_notice_srgba() -> [f32; 4] {
    [0.95, 0.68, 0.32, 1.0]
}

fn default_success_srgba() -> [f32; 4] {
    [0.28, 0.78, 0.65, 1.0]
}

fn default_library_warning_srgba() -> [f32; 4] {
    [0.94, 0.70, 0.34, 1.0]
}

fn default_panel_border_srgba() -> [f32; 4] {
    [0.18, 0.34, 0.39, 0.72]
}

fn default_panel_shadow_srgba() -> [f32; 4] {
    [0.0, 0.0, 0.0, 0.48]
}

fn default_panel_shadow_blur_px() -> f32 {
    30.0
}

fn default_panel_shadow_offset_y_px() -> f32 {
    12.0
}

fn default_button_border_srgba() -> [f32; 4] {
    [0.16, 0.31, 0.36, 0.78]
}

fn default_button_active_border_srgba() -> [f32; 4] {
    [0.28, 0.78, 0.77, 1.0]
}

fn default_danger_button_srgba() -> [f32; 4] {
    [0.31, 0.075, 0.095, 1.0]
}

fn default_danger_button_hover_srgba() -> [f32; 4] {
    [0.48, 0.105, 0.14, 1.0]
}

fn default_panel_radius_px() -> f32 {
    18.0
}

fn default_card_radius_px() -> f32 {
    12.0
}

fn default_world_card_height_px() -> f32 {
    116.0
}

fn default_world_card_gap_px() -> f32 {
    12.0
}

fn default_world_preview_size_px() -> f32 {
    84.0
}

fn default_world_description_size_px() -> f32 {
    11.0
}

fn default_world_metadata_size_px() -> f32 {
    9.0
}

fn default_world_empty_description() -> String {
    "No description supplied.".to_owned()
}

fn default_world_preview_label() -> String {
    "WORLD".to_owned()
}

fn default_world_version_prefix() -> String {
    "V".to_owned()
}

fn default_world_format_label() -> String {
    "FORMAT".to_owned()
}

fn default_world_regions_label() -> String {
    "REGIONS".to_owned()
}

fn default_world_objects_label() -> String {
    "OBJECTS".to_owned()
}

fn default_world_metadata_max_fields() -> usize {
    4
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MenuButtonDefinition {
    label: String,
    action: String,
}

impl MenuAsset {
    fn fallback() -> Self {
        let mut fallback: Self =
            serde_json::from_str(include_str!("../assets/ui/main_menu.json"))
            .expect("the embedded menu theme must be valid JSON");
        fallback.ensure_required_actions();
        fallback
    }

    fn ensure_required_actions(&mut self) {
        if !self.buttons.iter().any(|button| button.action == "resume") {
            self.buttons.push(MenuButtonDefinition {
                label: "RESUME".to_owned(),
                action: "resume".to_owned(),
            });
        }
        if !self.buttons.iter().any(|button| button.action == "quit") {
            self.buttons.push(MenuButtonDefinition {
                label: "QUIT".to_owned(),
                action: "quit".to_owned(),
            });
        }
    }

    fn validate(&self) -> Result<(), String> {
        if !matches!(self.schema_version, 1 | 2) {
            return Err(format!(
                "unsupported menu schema version {}",
                self.schema_version
            ));
        }
        if self.title.trim().is_empty() || self.subtitle.trim().is_empty() {
            return Err("menu title and subtitle cannot be empty".to_owned());
        }
        let presentation_labels = [
            &self.world_empty_description_text,
            &self.world_preview_fallback_label,
            &self.world_preview_version_prefix,
            &self.world_format_label,
            &self.world_regions_label,
            &self.world_objects_label,
            &self.empty_library_title,
            &self.empty_library_hint,
            &self.quit_confirm_title,
            &self.quit_confirm_message,
            &self.quit_confirm_label,
            &self.quit_cancel_label,
            &self.library_navigation_hint,
            &self.pause_controls_hint,
            &self.pause_title,
            &self.world_count_template,
            &self.library_warning_template,
        ];
        if presentation_labels
            .iter()
            .any(|label| label.trim().is_empty())
        {
            return Err("world-library presentation labels cannot be empty".to_owned());
        }
        if self.world_metadata_max_fields == 0 {
            return Err("world_metadata_max_fields must be greater than zero".to_owned());
        }
        if !self.world_count_template.contains("{count}")
            || !self.library_warning_template.contains("{count}")
        {
            return Err("world-count and library-warning templates must contain {count}".to_owned());
        }

        let colors = [
            self.background_srgba,
            self.panel_srgba,
            self.title_srgba,
            self.subtitle_srgba,
            self.button_srgba,
            self.button_hover_srgba,
            self.button_text_srgba,
            self.panel_border_srgba,
            self.panel_shadow_srgba,
            self.button_border_srgba,
            self.button_active_border_srgba,
            self.danger_button_srgba,
            self.danger_button_hover_srgba,
            self.modal_backdrop_srgba,
            self.notice_srgba,
            self.success_srgba,
            self.library_warning_srgba,
        ];
        if !colors
            .iter()
            .flatten()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        {
            return Err("menu colors must be finite sRGBA values in [0, 1]".to_owned());
        }

        let dimensions = [
            self.panel_width_px,
            self.panel_padding_px,
            self.vertical_gap_px,
            self.title_size_px,
            self.subtitle_size_px,
            self.button_width_px,
            self.button_height_px,
            self.button_font_size_px,
            self.world_card_height_px,
            self.world_card_gap_px,
            self.world_preview_size_px,
            self.world_description_size_px,
            self.world_metadata_size_px,
        ];
        if !dimensions
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
        {
            return Err("menu dimensions and font sizes must be positive and finite".to_owned());
        }
        if ![
            self.panel_radius_px,
            self.card_radius_px,
            self.panel_shadow_blur_px,
        ]
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
            || !self.panel_shadow_offset_y_px.is_finite()
        {
            return Err(
                "panel/card radii and shadow blur must be non-negative; shadow offset must be finite"
                    .to_owned(),
            );
        }
        if self.buttons.is_empty() {
            return Err("menu must define at least one button".to_owned());
        }

        let mut actions = BTreeSet::new();
        for button in &self.buttons {
            if button.label.trim().is_empty() {
                return Err("menu button labels cannot be empty".to_owned());
            }
            if !matches!(button.action.as_str(), "start" | "resume" | "quit") {
                return Err(format!(
                    "unsupported menu action '{}' (expected start, resume, or quit)",
                    button.action
                ));
            }
            if !actions.insert(button.action.as_str()) {
                return Err(format!("duplicate menu action '{}'", button.action));
            }
        }
        if !actions.contains("resume") || !actions.contains("quit") {
            return Err("menu must define both resume and quit actions".to_owned());
        }
        Ok(())
    }
}

fn setup_menu(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<RuntimeConfig>,
    library: Res<WorldLibrary>,
    mut state: ResMut<MenuState>,
    mut feedback: ResMut<MenuFeedback>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    let menu_path = config.paths.assets_dir.join("ui/main_menu.json");
    info!(
        "Initializing menu: world='{}', library_start={}, discovered_worlds={}, menu_asset='{}'",
        config.world.id,
        config.start_in_library,
        library.entries.len(),
        menu_path.display()
    );
    let menu = match load_menu(&menu_path) {
        Ok(menu) => {
            feedback.message.clear();
            feedback.tone = MenuFeedbackTone::Notice;
            menu
        }
        Err(error) => {
            error!(
                "Could not load menu presentation '{}': {error}. Using the built-in theme.",
                menu_path.display()
            );
            feedback.message =
                "The menu theme could not be loaded. The built-in theme is active.".to_owned();
            feedback.tone = MenuFeedbackTone::Notice;
            MenuAsset::fallback()
        }
    };

    state.visible = config.start_in_library;
    if state.visible {
        virtual_time.pause();
    } else {
        virtual_time.unpause();
    }

    let background_color = color_from_srgba(menu.background_srgba);
    let panel_color = color_from_srgba(menu.panel_srgba);
    let title_color = color_from_srgba(menu.title_srgba);
    let subtitle_color = color_from_srgba(menu.subtitle_srgba);
    let button_color = color_from_srgba(menu.button_srgba);
    let button_hover_color = color_from_srgba(menu.button_hover_srgba);
    let button_text_color = color_from_srgba(menu.button_text_srgba);
    let panel_border_color = color_from_srgba(menu.panel_border_srgba);
    let panel_shadow_color = color_from_srgba(menu.panel_shadow_srgba);
    let modal_backdrop_color = color_from_srgba(menu.modal_backdrop_srgba);
    let notice_color = color_from_srgba(menu.notice_srgba);
    let success_color = color_from_srgba(menu.success_srgba);
    let notice_colors = MenuNoticeColors {
        notice: notice_color,
        success: success_color,
    };
    let library_warning_color = color_from_srgba(menu.library_warning_srgba);
    let button_border_color = color_from_srgba(menu.button_border_srgba);
    let button_active_border_color = color_from_srgba(menu.button_active_border_srgba);
    let root_visibility = if state.visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    info!(
        "Menu asset loaded; scheduling UI entities with initial visibility {:?}",
        root_visibility
    );

    commands
        .spawn((
            MenuRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(background_color),
            root_visibility,
        ))
        .with_children(|root| {
            root.spawn((
                LibraryOnly,
                Visibility::Visible,
                Node {
                    width: Val::Percent(92.0),
                    max_width: Val::Px(menu.panel_width_px),
                    height: Val::Percent(88.0),
                    max_height: Val::Percent(88.0),
                    min_height: Val::Px(0.0),
                    padding: UiRect::all(Val::Px(menu.panel_padding_px)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    row_gap: Val::Px(menu.vertical_gap_px),
                    overflow: Overflow::hidden(),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(menu.panel_radius_px)),
                    ..default()
                },
                BorderColor::all(panel_border_color),
                BackgroundColor(panel_color),
                BoxShadow::new(
                    panel_shadow_color,
                    Val::Px(0.0),
                    Val::Px(menu.panel_shadow_offset_y_px),
                    Val::Px(0.0),
                    Val::Px(menu.panel_shadow_blur_px),
                ),
                TabGroup::modal(),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(menu.title.clone()),
                    TextFont {
                        font_size: FontSize::Px(menu.title_size_px),
                        ..default()
                    },
                    TextColor(title_color),
                ));
                panel.spawn((
                    Text::new(menu.subtitle.clone()),
                    TextFont {
                        font_size: FontSize::Px(menu.subtitle_size_px),
                        ..default()
                    },
                    TextColor(subtitle_color),
                ));
                panel.spawn((
                    Text::new(
                        menu.world_count_template
                            .replace("{count}", &library.entries.len().to_string()),
                    ),
                    TextFont {
                        font_size: FontSize::Px(menu.world_metadata_size_px),
                        ..default()
                    },
                    TextColor(subtitle_color),
                ));

                if !library.warnings.is_empty() {
                    panel.spawn((
                        Text::new(
                            menu.library_warning_template
                                .replace("{count}", &library.warnings.len().to_string()),
                        ),
                        TextFont {
                            font_size: FontSize::Px(menu.world_metadata_size_px),
                            ..default()
                        },
                        TextColor(library_warning_color),
                    ));
                }

                panel
                    .spawn((
                        WorldList,
                        Node {
                            width: Val::Percent(100.0),
                            min_height: Val::Px(0.0),
                            flex_grow: 1.0,
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(menu.world_card_gap_px),
                            overflow: Overflow::scroll_y(),
                            scrollbar_width: 7.0,
                            ..default()
                        },
                        ScrollPosition::default(),
                    ))
                    .with_children(|world_list| {
                if library.entries.is_empty() {
                    world_list
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                padding: UiRect::all(Val::Px(24.0)),
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::FlexStart,
                                row_gap: Val::Px(10.0),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(menu.card_radius_px)),
                                ..default()
                            },
                            BorderColor::all(panel_border_color),
                            BackgroundColor(button_color),
                        ))
                        .with_children(|empty| {
                            empty.spawn((
                                Text::new(menu.empty_library_title.clone()),
                                TextFont {
                                    font_size: FontSize::Px(menu.button_font_size_px),
                                    ..default()
                                },
                                TextColor(button_text_color),
                            ));
                            empty.spawn((
                                Text::new(menu.empty_library_hint.clone()),
                                TextFont {
                                    font_size: FontSize::Px(menu.world_description_size_px),
                                    ..default()
                                },
                                TextColor(subtitle_color),
                            ));
                        });
                }

                for (world_index, world) in library.entries.iter().enumerate() {
                    let preview = world
                        .preview_asset
                        .as_ref()
                        .map(|path| asset_server.load(path.clone()));
                    let schema_version = world.schema_version;
                    let preview_fallback_label = menu.world_preview_fallback_label.clone();
                    let preview_version_prefix = menu.world_preview_version_prefix.clone();
                    let description = if world.description.trim().is_empty() {
                        menu.world_empty_description_text.clone()
                    } else {
                        world.description.clone()
                    };
                    let metadata_lines =
                        world_metadata_lines(&world.metadata, menu.world_metadata_max_fields);
                    let format_summary = format!(
                        "{} v{}  |  {} {}  |  {} {}",
                        menu.world_format_label,
                        world.schema_version,
                        world.region_count,
                        menu.world_regions_label,
                        world.object_count,
                        menu.world_objects_label
                    );

                    world_list
                        .spawn((
                            Button,
                            TabIndex(world_index as i32),
                            Hovered::default(),
                            Node {
                                width: Val::Percent(100.0),
                                min_height: Val::Px(menu.world_card_height_px),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(menu.card_radius_px)),
                                padding: UiRect::all(Val::Px(12.0)),
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(menu.world_card_gap_px),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::FlexStart,
                                ..default()
                            },
                            BorderColor::all(button_border_color),
                            BackgroundColor(button_color),
                            WorldSelection(world.manifest_path.clone()),
                            MenuButtonColors {
                                normal: button_color,
                                hovered: button_hover_color,
                                border_normal: button_border_color,
                                border_active: button_active_border_color,
                            },
                            observe(handle_world_selection),
                        ))
                        .with_children(move |button| {
                            if let Some(preview) = preview.clone() {
                                button.spawn((
                                    ImageNode::new(preview),
                                    Node {
                                        width: Val::Px(menu.world_preview_size_px),
                                        height: Val::Px(menu.world_preview_size_px),
                                        border_radius: BorderRadius::all(Val::Px(menu.card_radius_px)),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                ));
                            } else {
                                button
                                    .spawn((
                                        Node {
                                            width: Val::Px(menu.world_preview_size_px),
                                            height: Val::Px(menu.world_preview_size_px),
                                            border_radius: BorderRadius::all(Val::Px(menu.card_radius_px)),
                                            flex_shrink: 0.0,
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::Center,
                                            ..default()
                                        },
                                        BackgroundColor(panel_color),
                                    ))
                                    .with_children(|tile| {
                                        tile.spawn((
                                            Text::new(format!(
                                                "{}\n{}{}",
                                                preview_fallback_label,
                                                preview_version_prefix,
                                                schema_version
                                            )),
                                            TextFont {
                                                font_size: FontSize::Px(
                                                    menu.world_metadata_size_px,
                                                ),
                                                ..default()
                                            },
                                            TextColor(subtitle_color),
                                        ));
                                    });
                            }

                            button
                                .spawn(Node {
                                    width: Val::Px(0.0),
                                    min_width: Val::Px(0.0),
                                    flex_grow: 1.0,
                                    flex_direction: FlexDirection::Column,
                                    justify_content: JustifyContent::Center,
                                    row_gap: Val::Px(4.0),
                                    ..default()
                                })
                                .with_children(move |details| {
                                    details.spawn((
                                        Text::new(format!("{} / {}", world.name, world.id)),
                                        TextFont {
                                            font_size: FontSize::Px(menu.button_font_size_px),
                                            ..default()
                                        },
                                        TextColor(button_text_color),
                                    ));
                                    details.spawn((
                                        Text::new(description.clone()),
                                        TextFont {
                                            font_size: FontSize::Px(menu.world_description_size_px),
                                            ..default()
                                        },
                                        TextColor(subtitle_color),
                                    ));
                                    details.spawn((
                                        Text::new(format_summary.clone()),
                                        TextFont {
                                            font_size: FontSize::Px(menu.world_metadata_size_px),
                                            ..default()
                                        },
                                        TextColor(subtitle_color),
                                    ));
                                    for metadata_line in &metadata_lines {
                                        details.spawn((
                                            Text::new(metadata_line.clone()),
                                            TextFont {
                                                font_size: FontSize::Px(
                                                    menu.world_metadata_size_px,
                                                ),
                                                ..default()
                                            },
                                            TextColor(subtitle_color),
                                        ));
                                    }
                                });
                        });
                }

                    });

                panel.spawn((
                    Text::new(menu.library_navigation_hint.clone()),
                    TextFont {
                        font_size: FontSize::Px(menu.world_metadata_size_px),
                        ..default()
                    },
                    TextColor(subtitle_color),
                ));

                panel.spawn((
                    MenuNotice,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(menu.world_description_size_px),
                        ..default()
                    },
                    TextColor(notice_color),
                    notice_colors,
                    Node {
                        width: Val::Percent(100.0),
                        min_height: Val::Px(18.0),
                        ..default()
                    },
                ));

                if let Some(quit) = menu.buttons.iter().find(|button| button.action == "quit") {
                    panel
                        .spawn((
                            Button,
                            TabIndex(library.entries.len() as i32),
                            Hovered::default(),
                            Node {
                                width: Val::Percent(100.0),
                                max_width: Val::Px(menu.button_width_px),
                                align_self: AlignSelf::Center,
                                height: Val::Px(menu.button_height_px),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(menu.card_radius_px)),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BorderColor::all(button_border_color),
                            BackgroundColor(button_color),
                            MenuAction(quit.action.clone()),
                            MenuButtonColors {
                                normal: button_color,
                                hovered: button_hover_color,
                                border_normal: button_border_color,
                                border_active: button_active_border_color,
                            },
                            observe(handle_menu_action),
                        ))
                        .with_children(|button| {
                            button.spawn((
                                Text::new(quit.label.clone()),
                                TextFont {
                                    font_size: FontSize::Px(menu.button_font_size_px),
                                    ..default()
                                },
                                TextColor(button_text_color),
                            ));
                        });
                }
            });

            root.spawn((
                SimulationOnly,
                TabGroup::modal(),
                Visibility::Hidden,
                Node {
                    width: Val::Percent(92.0),
                    max_width: Val::Px(menu.panel_width_px),
                    padding: UiRect::all(Val::Px(menu.panel_padding_px)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    row_gap: Val::Px(menu.vertical_gap_px),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(menu.panel_radius_px)),
                    ..default()
                },
                BorderColor::all(panel_border_color),
                BackgroundColor(panel_color),
                BoxShadow::new(
                    panel_shadow_color,
                    Val::Px(0.0),
                    Val::Px(menu.panel_shadow_offset_y_px),
                    Val::Px(0.0),
                    Val::Px(menu.panel_shadow_blur_px),
                ),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(menu.pause_title.clone()),
                    TextFont {
                        font_size: FontSize::Px(menu.title_size_px),
                        ..default()
                    },
                    TextColor(title_color),
                ));
                panel.spawn((
                    Text::new(menu.pause_controls_hint.clone()),
                    TextFont {
                        font_size: FontSize::Px(menu.subtitle_size_px),
                        ..default()
                    },
                    TextColor(subtitle_color),
                ));

                panel.spawn((
                    MenuNotice,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(menu.world_description_size_px),
                        ..default()
                    },
                    TextColor(notice_color),
                    notice_colors,
                    Node {
                        width: Val::Percent(100.0),
                        min_height: Val::Px(18.0),
                        ..default()
                    },
                ));

                for (index, definition) in menu
                    .buttons
                    .iter()
                    .filter(|button| matches!(button.action.as_str(), "resume" | "quit"))
                    .enumerate()
                {
                    panel
                        .spawn((
                            Button,
                            TabIndex(index as i32),
                            Hovered::default(),
                            Node {
                                width: Val::Percent(100.0),
                                max_width: Val::Px(menu.button_width_px),
                                align_self: AlignSelf::Center,
                                height: Val::Px(menu.button_height_px),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(menu.card_radius_px)),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BorderColor::all(button_border_color),
                            BackgroundColor(button_color),
                            MenuAction(definition.action.clone()),
                            MenuButtonColors {
                                normal: button_color,
                                hovered: button_hover_color,
                                border_normal: button_border_color,
                                border_active: button_active_border_color,
                            },
                            observe(handle_menu_action),
                        ))
                        .with_children(|button| {
                            button.spawn((
                                Text::new(definition.label.clone()),
                                TextFont {
                                    font_size: FontSize::Px(menu.button_font_size_px),
                                    ..default()
                                },
                                TextColor(button_text_color),
                            ));
                        });
                }
            });

            root.spawn((
                QuitConfirmationPanel,
                Visibility::Hidden,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::all(Val::Px(20.0)),
                    ..default()
                },
                BackgroundColor(modal_backdrop_color),
                TabGroup::modal(),
            ))
            .with_children(|overlay| {
                overlay.spawn((
                    Node {
                        width: Val::Percent(92.0),
                        max_width: Val::Px(560.0),
                        padding: UiRect::all(Val::Px(menu.panel_padding_px)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Stretch,
                        row_gap: Val::Px(menu.vertical_gap_px),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(menu.panel_radius_px)),
                        ..default()
                    },
                    BorderColor::all(panel_border_color),
                    BackgroundColor(panel_color),
                    BoxShadow::new(
                        panel_shadow_color,
                        Val::Px(0.0),
                        Val::Px(menu.panel_shadow_offset_y_px),
                        Val::Px(0.0),
                        Val::Px(menu.panel_shadow_blur_px),
                    ),
                ))
                .with_children(|dialog| {
                    dialog.spawn((
                        Text::new(menu.quit_confirm_title.clone()),
                        TextFont {
                            font_size: FontSize::Px(menu.title_size_px),
                            ..default()
                        },
                        TextColor(title_color),
                    ));
                    dialog.spawn((
                        Text::new(menu.quit_confirm_message.clone()),
                        TextFont {
                            font_size: FontSize::Px(menu.subtitle_size_px),
                            ..default()
                        },
                        TextColor(subtitle_color),
                    ));

                    dialog.spawn((
                        MenuNotice,
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(menu.world_description_size_px),
                            ..default()
                        },
                        TextColor(notice_color),
                        notice_colors,
                        Node {
                            width: Val::Percent(100.0),
                            min_height: Val::Px(18.0),
                            ..default()
                        },
                    ));

                    let danger_button_color = color_from_srgba(menu.danger_button_srgba);
                    let danger_button_hover_color =
                        color_from_srgba(menu.danger_button_hover_srgba);

                    for (index, (label, action)) in [
                        (menu.quit_cancel_label.clone(), "quit_cancel"),
                        (menu.quit_confirm_label.clone(), "quit_confirm"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let (normal_color, hovered_color) = if action == "quit_confirm" {
                            (danger_button_color, danger_button_hover_color)
                        } else {
                            (button_color, button_hover_color)
                        };

                        dialog
                            .spawn((
                                Button,
                                TabIndex(index as i32),
                                Hovered::default(),
                                Node {
                                    width: Val::Percent(100.0),
                                    min_height: Val::Px(menu.button_height_px),
                                    border: UiRect::all(Val::Px(1.0)),
                                    border_radius: BorderRadius::all(Val::Px(menu.card_radius_px)),
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                BorderColor::all(button_border_color),
                                BackgroundColor(normal_color),
                                MenuAction(action.to_owned()),
                                MenuButtonColors {
                                    normal: normal_color,
                                    hovered: hovered_color,
                                    border_normal: button_border_color,
                                    border_active: button_active_border_color,
                                },
                                observe(handle_menu_action),
                            ))
                            .with_children(|button| {
                                button.spawn((
                                    Text::new(label),
                                    TextFont {
                                        font_size: FontSize::Px(menu.button_font_size_px),
                                        ..default()
                                    },
                                    TextColor(button_text_color),
                                ));
                            });
                    }
                });
            });
        });
}

#[derive(Component)]
struct WorldSelection(PathBuf);

// This observer updates several Bevy resources as one atomic world-selection action.
#[allow(clippy::too_many_arguments)]
fn handle_world_selection(
    activate: On<Activate>,
    selections: Query<&WorldSelection>,
    mode: Res<State<AppMode>>,
    mut config: ResMut<RuntimeConfig>,
    mut world_state: ResMut<WorldStateFile>,
    mut save_target: ResMut<WorldSaveTarget>,
    mut journal: ResMut<DestructionJournal>,
    mut gravity: ResMut<Gravity>,
    mut next_state: ResMut<NextState<AppMode>>,
    mut menu_state: ResMut<MenuState>,
    mut feedback: ResMut<MenuFeedback>,
    mut input_focus: ResMut<InputFocus>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    if *mode.get() != AppMode::WorldLibrary {
        return;
    }

    let Ok(selection) = selections.get(activate.entity) else {
        return;
    };

    let data_dir = config.paths.data_dir.clone();
    let assets_dir = config.paths.assets_dir.clone();
    let project_root = config.paths.project_root.clone();
    let mut selected = match RuntimeConfig::load(data_dir, assets_dir, selection.0.clone()) {
        Ok(config) => config,
        Err(error) => {
            error!(
                "Could not load selected world '{}': {error:#}",
                selection.0.display()
            );
            feedback.message = format!(
                "Could not open '{}'. Check the application log for details.",
                selection.0.display()
            );
            feedback.tone = MenuFeedbackTone::Notice;
            return;
        }
    };
    selected.paths.project_root = project_root;

    let selected_state =
        match WorldStateFile::load_or_default(&selected.paths.world_state, &selected.world.id) {
            Ok(state) => state,
            Err(error) => {
                error!(
                    "Could not load save state for world '{}': {error:#}",
                    selected.world.id
                );
                feedback.message = format!(
                    "Could not load the save for '{}'. Check the application log for details.",
                    selected.world.name
                );
                feedback.tone = MenuFeedbackTone::Notice;
                return;
            }
        };

    gravity.0 = Vec3::from_array(
        selected
            .world
            .gravity_m_s2
            .unwrap_or(selected.engine.physics.gravity_m_s2),
    );
    save_target.world_id.clone_from(&selected.world.id);
    save_target
        .state_path
        .clone_from(&selected.paths.world_state);
    journal.fractured.clone_from(&selected_state.fractured);
    journal
        .broken_bonds
        .clone_from(&selected_state.broken_bonds);
    journal
        .damaged_bonds
        .clone_from(&selected_state.damaged_bonds);
    *world_state = selected_state;
    *config = selected;
    feedback.message.clear();
    input_focus.clear();

    menu_state.visible = false;
    virtual_time.unpause();
    next_state.set(AppMode::Simulation);
    info!(
        "Selected world '{}'. Starting simulation.",
        config.world.name
    );
}

fn load_menu(path: &Path) -> Result<MenuAsset, String> {
    let source =
        std::fs::read_to_string(path).map_err(|error| format!("failed to read file: {error}"))?;
    let mut menu: MenuAsset =
        serde_json::from_str(&source).map_err(|error| format!("invalid menu JSON: {error}"))?;
    menu.ensure_required_actions();
    menu.validate()?;
    Ok(menu)
}

fn handle_menu_action(
    activate: On<Activate>,
    actions: Query<(Entity, &MenuAction)>,
    mode: Res<State<AppMode>>,
    mut state: ResMut<MenuState>,
    mut confirmation: ResMut<QuitConfirmationState>,
    mut input_focus: ResMut<InputFocus>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let Ok((_, action)) = actions.get(activate.entity) else {
        return;
    };

    match action.0.as_str() {
        "start" | "resume" => {
            state.visible = false;
            input_focus.clear();
            virtual_time.unpause();
        }
        "quit" if *mode.get() == AppMode::Simulation => {
            confirmation.visible = true;
            confirmation.return_focus = Some(activate.entity);
            if let Some((cancel, _)) = actions.iter().find(|(_, action)| action.0 == "quit_cancel") {
                input_focus.set(cancel, FocusCause::Navigated);
            }
        }
        "quit" => {
            app_exit.write(AppExit::Success);
        }
        "quit_cancel" => {
            confirmation.visible = false;
            if let Some(previous) = confirmation.return_focus.take() {
                input_focus.set(previous, FocusCause::Navigated);
            } else {
                input_focus.clear();
            }
        }
        "quit_confirm" => {
            app_exit.write(AppExit::Success);
        }
        _ => unreachable!("menu actions are validated during asset loading"),
    }
}

fn sync_quit_confirmation_visibility(
    confirmation: Res<QuitConfirmationState>,
    mut panels: Query<&mut Visibility, With<QuitConfirmationPanel>>,
) {
    for mut visibility in &mut panels {
        visibility.set_if_neq(if confirmation.visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
}

fn sync_menu_feedback(
    feedback: Res<MenuFeedback>,
    mut notices: Query<
        (&mut Text, &mut TextColor, &MenuNoticeColors),
        With<MenuNotice>,
    >,
) {
    if !feedback.is_changed() {
        return;
    }

    for (mut text, mut text_color, colors) in &mut notices {
        text.0.clone_from(&feedback.message);
        text_color.0 = match feedback.tone {
            MenuFeedbackTone::Notice => colors.notice,
            MenuFeedbackTone::Success => colors.success,
        };
    }
}

fn scroll_position_after_delta(
    current: f32,
    delta: f32,
    visible_height: f32,
    content_height: f32,
    inverse_scale_factor: f32,
) -> f32 {
    let max_offset = (content_height - visible_height).max(0.0) * inverse_scale_factor;
    (current - delta).clamp(0.0, max_offset)
}

fn update_world_list_scroll(
    mut mouse_wheel_reader: MessageReader<MouseWheel>,
    hover_map: Res<HoverMap>,
    mut scrollable: Query<(&mut ScrollPosition, &ComputedNode), With<WorldList>>,
) {
    for wheel in mouse_wheel_reader.read() {
        let dy = match wheel.unit {
            MouseScrollUnit::Line => wheel.y * 24.0,
            MouseScrollUnit::Pixel => wheel.y,
        };

        for pointer_map in hover_map.values() {
            for entity in pointer_map.keys() {
                let Ok((mut position, computed)) = scrollable.get_mut(*entity) else {
                    continue;
                };
                position.0.y = scroll_position_after_delta(
                    position.0.y,
                    dy,
                    computed.size().y,
                    computed.content_size().y,
                    computed.inverse_scale_factor,
                );
            }
        }
    }
}

fn toggle_menu_with_escape(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<State<AppMode>>,
    mut state: ResMut<MenuState>,
    actions: Query<(Entity, &MenuAction)>,
    mut confirmation: ResMut<QuitConfirmationState>,
    mut input_focus: ResMut<InputFocus>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }

    if confirmation.visible {
        confirmation.visible = false;
        if let Some(previous) = confirmation.return_focus.take() {
            input_focus.set(previous, FocusCause::Navigated);
        } else {
            input_focus.clear();
        }
        return;
    }

    if *mode.get() == AppMode::WorldLibrary {
        return;
    }

    state.visible = !state.visible;
    if state.visible {
        virtual_time.pause();
        if let Some((resume, _)) = actions.iter().find(|(_, action)| action.0 == "resume") {
            input_focus.set(resume, FocusCause::Navigated);
        }
    } else {
        input_focus.clear();
        virtual_time.unpause();
    }
}

fn update_button_styles(
    input_focus: Res<InputFocus>,
    focus_visible: Res<InputFocusVisible>,
    mut buttons: Query<
        (
            Entity,
            &Hovered,
            Has<Pressed>,
            &MenuButtonColors,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<Button>,
    >,
) {
    for (entity, hovered, pressed, colors, mut background, mut border) in &mut buttons {
        let active = pressed || hovered.get() || (focus_visible.0 && input_focus.get() == Some(entity));
        background.0 = if active { colors.hovered } else { colors.normal };
        *border = BorderColor::all(if active {
            colors.border_active
        } else {
            colors.border_normal
        });
    }
}

type RootVisibilityQuery<'world, 'state> = Query<
    'world,
    'state,
    &'static mut Visibility,
    (
        With<MenuRoot>,
        Without<LibraryOnly>,
        Without<SimulationOnly>,
    ),
>;
type LibraryVisibilityQuery<'world, 'state> = Query<
    'world,
    'state,
    &'static mut Visibility,
    (
        With<LibraryOnly>,
        Without<SimulationOnly>,
        Without<MenuRoot>,
    ),
>;
type SimulationVisibilityQuery<'world, 'state> = Query<
    'world,
    'state,
    &'static mut Visibility,
    (
        With<SimulationOnly>,
        Without<LibraryOnly>,
        Without<MenuRoot>,
    ),
>;

fn sync_menu_visibility(
    state: Res<MenuState>,
    mode: Res<State<AppMode>>,
    mut roots: RootVisibilityQuery<'_, '_>,
    mut library_panels: LibraryVisibilityQuery<'_, '_>,
    mut simulation_panels: SimulationVisibilityQuery<'_, '_>,
) {
    let (root_visibility, library_visibility, simulation_visibility) =
        menu_visibility_targets(*mode.get() == AppMode::WorldLibrary, state.visible);
    if state.is_changed() || mode.is_changed() {
        info!(
            "Menu state changed: mode={:?}, menu_visible={}, root={:?}, library={:?}, pause={:?}",
            mode.get(),
            state.visible,
            root_visibility,
            library_visibility,
            simulation_visibility
        );
    }

    // Evaluate the complete state every frame. This keeps the top-level overlay and its
    // explicitly-visible children in sync even when a state transition and activation occur
    // in the same frame.
    for mut visibility in &mut roots {
        visibility.set_if_neq(root_visibility);
    }
    for mut visibility in &mut library_panels {
        visibility.set_if_neq(library_visibility);
    }
    for mut visibility in &mut simulation_panels {
        visibility.set_if_neq(simulation_visibility);
    }
}

fn menu_visibility_targets(
    in_library: bool,
    menu_visible: bool,
) -> (Visibility, Visibility, Visibility) {
    let root = if in_library || menu_visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    let library = if in_library {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    let simulation = if !in_library && menu_visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    (root, library, simulation)
}

fn world_metadata_lines(
    metadata: &BTreeMap<String, serde_json::Value>,
    maximum_fields: usize,
) -> Vec<String> {
    metadata
        .iter()
        .take(maximum_fields)
        .filter_map(|(key, value)| {
            let display_value = metadata_value_to_display(value);
            (!display_value.is_empty())
                .then(|| format!("{}: {display_value}", key.replace('_', " ")))
        })
        .collect()
}

fn metadata_value_to_display(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => value.to_string(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => values
            .iter()
            .map(metadata_value_to_display)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(key, value)| {
                let display_value = metadata_value_to_display(value);
                if display_value.is_empty() {
                    key.clone()
                } else {
                    format!("{key}: {display_value}")
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn color_from_srgba(value: [f32; 4]) -> Color {
    Color::srgba(value[0], value[1], value[2], value[3])
}

#[cfg(test)]
mod tests {
    use super::{
        MenuAsset, MenuButtonDefinition, default_world_empty_description,
        default_world_format_label, default_world_metadata_max_fields, default_world_objects_label,
        default_world_preview_label, default_world_regions_label, default_world_version_prefix,
        menu_visibility_targets, scroll_position_after_delta, world_metadata_lines,
    };
    use bevy::prelude::Visibility;
    use std::collections::BTreeMap;

    fn valid_menu() -> MenuAsset {
        MenuAsset {
            schema_version: 1,
            title: "Title".to_owned(),
            subtitle: "Subtitle".to_owned(),
            empty_library_title: default_empty_library_title(),
            empty_library_hint: default_empty_library_hint(),
            quit_confirm_title: default_quit_confirm_title(),
            quit_confirm_message: default_quit_confirm_message(),
            quit_confirm_label: default_quit_confirm_label(),
            quit_cancel_label: default_quit_cancel_label(),
            library_navigation_hint: default_library_navigation_hint(),
            pause_controls_hint: default_pause_controls_hint(),
            pause_title: default_pause_title(),
            world_count_template: default_world_count_template(),
            library_warning_template: default_library_warning_template(),
            modal_backdrop_srgba: default_modal_backdrop_srgba(),
            notice_srgba: default_notice_srgba(),
            success_srgba: default_success_srgba(),
            library_warning_srgba: default_library_warning_srgba(),
            background_srgba: [0.0, 0.0, 0.0, 1.0],
            panel_srgba: [0.1, 0.1, 0.1, 1.0],
            title_srgba: [1.0; 4],
            subtitle_srgba: [1.0; 4],
            button_srgba: [0.1, 0.1, 0.1, 1.0],
            button_hover_srgba: [0.2, 0.2, 0.2, 1.0],
            button_text_srgba: [1.0; 4],
            panel_border_srgba: default_panel_border_srgba(),
            panel_shadow_srgba: default_panel_shadow_srgba(),
            panel_shadow_blur_px: default_panel_shadow_blur_px(),
            panel_shadow_offset_y_px: default_panel_shadow_offset_y_px(),
            button_border_srgba: default_button_border_srgba(),
            button_active_border_srgba: default_button_active_border_srgba(),
            danger_button_srgba: default_danger_button_srgba(),
            danger_button_hover_srgba: default_danger_button_hover_srgba(),
            panel_width_px: 500.0,
            panel_padding_px: 32.0,
            vertical_gap_px: 12.0,
            title_size_px: 32.0,
            subtitle_size_px: 12.0,
            button_width_px: 300.0,
            button_height_px: 48.0,
            button_font_size_px: 16.0,
            world_card_height_px: 116.0,
            world_card_gap_px: 12.0,
            world_preview_size_px: 84.0,
            world_description_size_px: 11.0,
            world_metadata_size_px: 9.0,
            world_empty_description_text: default_world_empty_description(),
            world_preview_fallback_label: default_world_preview_label(),
            world_preview_version_prefix: default_world_version_prefix(),
            world_format_label: default_world_format_label(),
            world_regions_label: default_world_regions_label(),
            world_objects_label: default_world_objects_label(),
            world_metadata_max_fields: default_world_metadata_max_fields(),
            panel_radius_px: default_panel_radius_px(),
            card_radius_px: default_card_radius_px(),
            buttons: vec![
                MenuButtonDefinition {
                    label: "Resume".to_owned(),
                    action: "resume".to_owned(),
                },
                MenuButtonDefinition {
                    label: "Quit".to_owned(),
                    action: "quit".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn world_library_visibility_is_independent_of_the_pause_menu_flag() {
        assert_eq!(
            menu_visibility_targets(true, false),
            (Visibility::Visible, Visibility::Visible, Visibility::Hidden)
        );
    }

    #[test]
    fn selecting_a_world_hides_both_the_overlay_and_library_panel() {
        assert_eq!(
            menu_visibility_targets(false, false),
            (Visibility::Hidden, Visibility::Hidden, Visibility::Hidden)
        );
    }

    #[test]
    fn simulation_pause_shows_only_the_pause_panel() {
        assert_eq!(
            menu_visibility_targets(false, true),
            (Visibility::Visible, Visibility::Hidden, Visibility::Visible)
        );
    }

    #[test]
    fn legacy_schema_one_menu_defaults_new_library_presentation_fields() {
        let legacy = serde_json::json!({
            "schema_version": 1,
            "title": "Legacy menu",
            "subtitle": "Legacy subtitle",
            "background_srgba": [0.0, 0.0, 0.0, 1.0],
            "panel_srgba": [0.1, 0.1, 0.1, 1.0],
            "title_srgba": [1.0, 1.0, 1.0, 1.0],
            "subtitle_srgba": [1.0, 1.0, 1.0, 1.0],
            "button_srgba": [0.1, 0.1, 0.1, 1.0],
            "button_hover_srgba": [0.2, 0.2, 0.2, 1.0],
            "button_text_srgba": [1.0, 1.0, 1.0, 1.0],
            "panel_width_px": 500.0,
            "panel_padding_px": 32.0,
            "vertical_gap_px": 12.0,
            "title_size_px": 32.0,
            "subtitle_size_px": 12.0,
            "button_width_px": 300.0,
            "button_height_px": 48.0,
            "button_font_size_px": 16.0,
            "buttons": [{"label": "QUIT", "action": "quit"}]
        });

        let mut menu: MenuAsset = serde_json::from_value(legacy).unwrap();
        menu.ensure_required_actions();
        assert!(menu.validate().is_ok());
        assert!(menu.buttons.iter().any(|button| button.action == "resume"));
        assert_eq!(menu.world_card_height_px, 116.0);
        assert_eq!(menu.world_preview_fallback_label, "WORLD");
        assert_eq!(menu.world_metadata_max_fields, 4);
        assert_eq!(menu.empty_library_title, "YOUR WORLD LIBRARY IS EMPTY");
        assert_eq!(menu.panel_radius_px, 18.0);
        assert_eq!(menu.card_radius_px, 12.0);
        assert_eq!(menu.panel_shadow_blur_px, 30.0);
        assert_eq!(menu.panel_shadow_offset_y_px, 12.0);
        assert_eq!(menu.quit_confirm_title, "UNSAVED SIMULATION");
        assert_eq!(menu.quit_confirm_label, "QUIT ANYWAY");
        assert_eq!(menu.quit_cancel_label, "KEEP PLAYING");
        assert_eq!(menu.danger_button_srgba, [0.31, 0.075, 0.095, 1.0]);
        assert_eq!(menu.library_navigation_hint, default_library_navigation_hint());
        assert_eq!(menu.pause_title, "SIMULATION PAUSED");
        assert_eq!(menu.world_count_template, "{count} WORLDS AVAILABLE");
        assert_eq!(menu.modal_backdrop_srgba, default_modal_backdrop_srgba());
        assert_eq!(menu.success_srgba, [0.28, 0.78, 0.65, 1.0]);
        assert_eq!(menu.quit_confirm_title, "UNSAVED SIMULATION");
        assert_eq!(menu.quit_cancel_label, "KEEP PLAYING");
        assert_eq!(menu.danger_button_srgba, [0.31, 0.075, 0.095, 1.0]);
    }

    #[test]
    fn world_metadata_arrays_render_as_compact_readable_rows() {
        let metadata = BTreeMap::from([
            ("author".to_owned(), serde_json::json!("sample content")),
            (
                "features".to_owned(),
                serde_json::json!(["rigid bodies", "bonded structures", "chunk streaming"]),
            ),
        ]);

        assert_eq!(
            world_metadata_lines(&metadata, 2),
            [
                "author: sample content",
                "features: rigid bodies, bonded structures, chunk streaming"
            ]
        );
        assert_eq!(world_metadata_lines(&metadata, 1).len(), 1);
    }

    #[test]
    fn scroll_position_clamps_to_content_and_respects_ui_scale() {
        let next = scroll_position_after_delta(150.0, 80.0, 300.0, 1100.0, 0.5);
        assert_eq!(next, 70.0);

        let at_start = scroll_position_after_delta(0.0, 50.0, 300.0, 1100.0, 0.5);
        assert_eq!(at_start, 0.0);

        let at_end = scroll_position_after_delta(390.0, -50.0, 300.0, 1100.0, 0.5);
        assert_eq!(at_end, 400.0);
    }

    #[test]
    fn fallback_theme_is_valid_and_retains_essential_actions() {
        let fallback = MenuAsset::fallback();
        assert!(fallback.validate().is_ok());
        assert!(fallback.buttons.iter().any(|button| button.action == "resume"));
        assert!(fallback.buttons.iter().any(|button| button.action == "quit"));
    }

    #[test]
    fn menu_theme_allows_flat_panels_and_disabled_shadows() {
        let mut menu = valid_menu();
        menu.panel_radius_px = 0.0;
        menu.card_radius_px = 0.0;
        menu.panel_shadow_blur_px = 0.0;
        menu.panel_shadow_offset_y_px = 0.0;
        assert!(menu.validate().is_ok());
    }

    #[test]
    fn menu_asset_requires_unique_resume_and_quit_actions() {
        let mut menu = valid_menu();
        menu.buttons.pop();
        assert!(menu.validate().is_err());

        let mut menu = valid_menu();
        menu.buttons.push(MenuButtonDefinition {
            label: "Another quit".to_owned(),
            action: "quit".to_owned(),
        });
        assert!(menu.validate().is_err());
    }

    #[test]
    fn menu_asset_rejects_unknown_actions_and_invalid_dimensions() {
        let mut menu = valid_menu();
        assert!(menu.validate().is_ok());

        menu.buttons[0].action = "spawn_admin".to_owned();
        assert!(menu.validate().is_err());

        let mut menu = valid_menu();
        menu.button_width_px = f32::NAN;
        assert!(menu.validate().is_err());
    }
}
