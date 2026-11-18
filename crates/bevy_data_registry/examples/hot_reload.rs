//! Hot reload: edit `assets/hot_reload/items.ron` (or add another
//! `items.ron` in a folder next to it) while this runs, and the definitions are
//! indexed and validated again, and `DataReloaded` sent. Needs the `dev`
//! feature.
//!
//! ```sh
//! cargo run --example hot_reload --features dev
//! ```

use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bevy_data_registry::prelude::*;
use serde::Deserialize;

#[derive(Definition, Deserialize)]
#[definition(file = "items")]
struct Item {
    id: String,
    price: u32,
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            // Headless: a frame every 16ms, not as fast as it can.
            DefaultPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
            DataRegistryPlugin {
                path: "hot_reload".into(),
                ..default()
            },
        ))
        .register_definition::<Item>()
        .add_systems(OnEnter(DataState::Loaded), show)
        // After the pipeline, which sends it in `Update`.
        .add_systems(PostUpdate, show.run_if(on_message::<DataReloaded>))
        .run()
}

/// Every item's price: on load, and after every reload.
fn show(items: Definitions<Item>) {
    let mut all: Vec<(&String, &Item)> = items.iter().collect();
    all.sort_by_key(|(id, _)| *id);
    let prices: Vec<String> = all
        .iter()
        .map(|(id, item)| format!("{id} {}", item.price))
        .collect();
    info!("prices: {}", prices.join(", "));
}
