mod catalog;
mod menu;
mod water;

pub use catalog::{ActiveWorld, CharacterStudioWorkspace, EditorProjectRoot, WorldCatalog};
use bevy::prelude::*;

use crate::{config::RuntimeConfig, persistence::WorldStateFile};

#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EditorScreen {
    #[default]
    MainMenu,
    WorldBrowser,
    InWorld,
}

#[derive(Clone, Debug)]
pub enum MenuAction {
    Disabled,
    OpenWorldBrowser,
    BackToMainMenu,
    LoadWorld(String),
    CreateWorld,
}

#[derive(Resource, Default)]
struct PendingMenuCommand(Option<MenuAction>);

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        let root = app
            .world()
            .resource::<RuntimeConfig>()
            .paths
            .root_dir
            .clone();

        app.insert_resource(EditorProjectRoot(root))
            .init_state::<EditorScreen>()
            .init_resource::<WorldCatalog>()
            .init_resource::<ActiveWorld>()
            .init_resource::<CharacterStudioWorkspace>()
            .init_resource::<PendingMenuCommand>()
            .init_resource::<water::RippleField>()
            .add_systems(Startup, catalog::load_world_catalog)
            .add_systems(OnEnter(EditorScreen::MainMenu), menu::spawn_main_menu)
            .add_systems(OnEnter(EditorScreen::WorldBrowser), menu::spawn_world_browser)
            .add_systems(
                OnExit(EditorScreen::MainMenu),
                menu::despawn_menu_scene,
            )
            .add_systems(
                OnExit(EditorScreen::WorldBrowser),
                menu::despawn_menu_scene,
            )
            .add_systems(
                Update,
                (
                    menu::interact_callouts,
                    catalog::process_menu_command,
                    water::animate_water,
                )
                    .chain()
                    .run_if(in_state(EditorScreen::MainMenu)),
            )
            .add_systems(
                Update,
                (
                    menu::interact_callouts,
                    catalog::process_menu_command,
                    water::animate_water,
                )
                    .chain()
                    .run_if(in_state(EditorScreen::WorldBrowser)),
            )
            .add_systems(
                Update,
                return_to_menu.run_if(in_state(EditorScreen::InWorld)),
            );

        if app.world().contains_resource::<WorldStateFile>() {
            info!("Editor state machine initialized");
        }
    }
}

fn return_to_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut next_screen: ResMut<NextState<EditorScreen>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next_screen.set(EditorScreen::MainMenu);
    }
}
