mod contacts;

pub use contacts::ContactLedger;

use bevy::prelude::*;

pub struct ContactPlugin;

impl Plugin for ContactPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ContactLedger>()
            .add_systems(Update, contacts::collect_contact_transitions);
    }
}
