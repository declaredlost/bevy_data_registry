//! Kinds of definitions, each read from the files it claims, a file per
//! kind in any folder, and looked up by id once loaded.
//!
//! ```sh
//! cargo run --example basic
//! ```

use bevy::prelude::*;
use bevy_data_registry::prelude::*;
use serde::Deserialize;

/// Something to pick up: in every `items.ron`, and every
/// `<anything>.items.ron`.
#[derive(Definition, Debug, Deserialize)]
#[definition(file = "items", extension = "items")]
struct Item {
    id: String,
    price: u32,
}

/// Something to fight: in every `monsters.ron`, whatever the type's name.
#[derive(Definition, Debug, Deserialize)]
#[definition(file = "monsters")]
struct Creature {
    id: String,
    health: u32,
    drops: Vec<String>,
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DataRegistryPlugin {
                path: "basic".into(),
                ..default()
            },
        ))
        .register_definition::<Item>()
        .register_definition::<Creature>()
        .add_systems(OnEnter(DataState::Loaded), (show, exit).chain())
        .run()
}

/// Every monster, and what its drops are worth.
fn show(monsters: Definitions<Creature>, items: Definitions<Item>) {
    let mut ids: Vec<&String> = monsters.ids().collect();
    ids.sort();
    for id in ids {
        let monster = &monsters.get(id).expect("listed");
        let worth: u64 = monster
            .drops
            .iter()
            .filter_map(|drop| items.get(drop))
            .map(|item| u64::from(item.price))
            .sum();
        info!(
            "{id}: {} health, drops {:?} worth {worth} (from {})",
            monster.health,
            monster.drops,
            monsters.path(id).expect("listed"),
        );
    }
    info!(
        "gold is worth {}",
        items.get("gold").map_or(0, |gold| gold.price)
    );
}

fn exit(mut exit: MessageWriter<AppExit>) {
    exit.write(AppExit::Success);
}
