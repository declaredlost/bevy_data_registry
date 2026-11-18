//! Checks across kinds: a system in `DataSystems::Validate` pushes what's
//! wrong to `DataErrors`. On first load any error fails loading: logged,
//! `DataState::Failed`, and the app told to exit with an error, which this
//! example's data is made to do.
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
    id: String,
    #[expect(dead_code, reason = "read from the data, not by this example")]
    price: u32,
}

#[derive(Definition, Deserialize)]
#[definition(file = "monsters")]
struct Monster {
    id: String,
    drops: Vec<String>,
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
        .add_systems(Update, drops_exist.in_set(DataSystems::Validate))
        .add_systems(OnEnter(DataState::Loaded), || {
            info!("everything checks out");
        })
        .add_systems(OnEnter(DataState::Failed), || {
            warn!("loading failed, as this example's data is made to");
        })
        .run()
}

/// Every monster's drops are items.
fn drops_exist(
    monsters: Definitions<Monster>,
    items: Definitions<Item>,
    mut errors: ResMut<DataErrors>,
) {
    for (id, monster) in &monsters {
        for drop in monster.drops.iter().filter(|drop| !items.contains(*drop)) {
            errors.push(format!(
                "Monster `{id}` ({}) drops `{drop}`, which isn't an item",
                monsters.path(id).expect("listed"),
            ));
        }
    }
}
