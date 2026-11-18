//! Kinds of definitions, each read from the files it claims, a file per
//! kind in any folder, and looked up by id once loaded. A creature's drops
//! are `Id<Item>`s: each checked to be an item as the data loads.
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
    id: Id<Item>,
    price: u32,
}

/// An id known in code: the same as the one read from `"gold"`.
const GOLD: Id<Item> = Id::from_name("gold");

/// Something to fight: in every `monsters.ron`, whatever the type's name.
#[derive(Definition, Debug, Deserialize)]
#[definition(file = "monsters")]
struct Creature {
    id: Id<Creature>,
    health: u32,
    drops: Vec<Id<Item>>,
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
    let mut ids: Vec<&Id<Creature>> = monsters.ids().collect();
    // `Id`s show as their hash: names are what they were read from.
    ids.sort_by_key(|id| monsters.name(id));
    for id in ids {
        let monster = &monsters.get(id).expect("listed");
        let drops: Vec<&str> = monster
            .drops
            .iter()
            .filter_map(|drop| items.name(drop))
            .collect();
        let worth: u64 = monster
            .drops
            .iter()
            .filter_map(|drop| items.get(drop))
            .map(|item| u64::from(item.price))
            .sum();
        info!(
            "{}: {} health, drops {drops:?} worth {worth} (from {})",
            monsters.name(id).expect("read"),
            monster.health,
            monsters.path(id).expect("listed"),
        );
    }
    info!(
        "gold is worth {}",
        items.get(&GOLD).map_or(0, |gold| gold.price)
    );
}

fn exit(mut exit: MessageWriter<AppExit>) {
    exit.write(AppExit::Success);
}
