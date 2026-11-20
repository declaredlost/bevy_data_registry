//! Checks across definitions: every `Id` in a definition, but its own, its
//! base and fields marked `#[definition(unchecked)]`, is checked to name a
//! definition by the crate, and a system of the app's own in
//! `DataSystems::Validate` pushes what else is wrong to `DataErrors`. On
//! first load any error fails loading: logged, `DataState::Failed`, and the
//! app told to exit with an error, which this example's data is made to
//! do, twice.
//!
//! ```sh
//! cargo run --example validation
//! ```

use bevy::prelude::*;
use bevy_data_registry::prelude::*;
use serde::Deserialize;

#[derive(Definition, Deserialize)]
#[definition(file = "items")]
struct Item {
    id: Id<Item>,
    price: u32,
}

#[derive(Definition, Deserialize)]
#[definition(file = "monsters")]
struct Monster {
    id: Id<Monster>,
    /// Each checked to be an item.
    drops: Vec<Id<Item>>,
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DataRegistryPlugin {
                path: "validation".into(),
                ..default()
            },
        ))
        .register_definition::<Item>()
        .register_definition::<Monster>()
        .add_systems(Update, priced.in_set(DataSystems::Validate))
        .add_systems(OnEnter(DataState::Loaded), || {
            info!("everything checks out");
        })
        .add_systems(OnEnter(DataState::Failed), |errors: Res<DataErrors>| {
            warn!(
                "loading failed with {} errors, as this example's data is made to",
                errors.iter().count()
            );
        })
        .run()
}

/// Every item is worth something: a check of the app's own.
fn priced(items: Definitions<Item>, mut errors: ResMut<DataErrors>) {
    for (id, _) in items.iter().filter(|(_, item)| item.price == 0) {
        errors.push(format!(
            "Item `{}` ({}) is worth nothing",
            items.name(id).expect("read"),
            items.path(id).expect("listed"),
        ));
    }
}
