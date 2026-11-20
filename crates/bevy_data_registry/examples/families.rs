//! Families: a definition builds on another, its base, and takes from it
//! what it doesn't say; a family itself is only built on, never read.
//!
//! ```sh
//! cargo run --example families
//! ```

use std::sync::Arc;

use bevy::prelude::*;
use bevy_data_registry::format::Ron;
use bevy_data_registry::prelude::*;
use serde::Deserialize;

/// A weapon, whose fields are optional so it can leave them to its base;
/// in every file under `weapons/`.
#[derive(Definition, Debug, Clone, Deserialize)]
#[definition(folder = "weapons")]
struct Weapon {
    id: Id<Weapon>,
    #[definition(base)]
    #[serde(default)]
    base: Option<Id<Weapon>>,
    #[definition(family)]
    #[serde(default)]
    family: bool,
    #[serde(default)]
    damage: Option<u32>,
    #[serde(default)]
    speed: Option<f32>,
    #[serde(default)]
    price: Option<u32>,
}

/// What a weapon leaves out, it takes from its base, already whole.
impl Inherit for Weapon {
    fn inherit(&mut self, base: &Self) {
        self.damage = self.damage.or(base.damage);
        self.speed = self.speed.or(base.speed);
        self.price = self.price.or(base.price);
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DataRegistryPlugin {
                path: "families".into(),
                formats: vec![Arc::new(Ron {
                    options: ron::Options::default()
                        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME),
                    ..default()
                })],
                ..default()
            },
        ))
        .register_definition::<Weapon>()
        .add_systems(OnEnter(DataState::Loaded), (show, exit).chain())
        .run()
}

/// Every weapon, whole; the families aren't among them.
fn show(weapons: Definitions<Weapon>) {
    let mut all: Vec<(&str, &Weapon)> = weapons
        .iter()
        .filter_map(|(id, weapon)| Some((weapons.name(id)?, weapon)))
        .collect();
    all.sort_by_key(|(name, _)| *name);
    for (name, weapon) in all {
        info!(
            "{name}: damage {:?}, speed {:?}, price {:?}",
            weapon.damage, weapon.speed, weapon.price
        );
    }
    assert!(
        weapons.get(&Id::from_name("blade")).is_none(),
        "a family is never read"
    );
}

fn exit(mut exit: MessageWriter<AppExit>) {
    exit.write(AppExit::Success);
}
