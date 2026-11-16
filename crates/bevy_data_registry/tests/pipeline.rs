//! The whole pipeline, as an app runs it: listing the files (in kind
//! folders and named after kinds), loading, indexing, families made whole,
//! and `Defs` reading them.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_data_registry::{
    DataApp, DataRegistryPlugin, DataState, Definition, Defs, FamilyDefinition,
};
use bevy_harness::TestApp;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Thing {
    id: String,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    r#abstract: bool,
    #[serde(default)]
    weight: Option<u32>,
}

impl Definition for Thing {
    type Id = String;

    const FOLDER: &'static str = "things";
    const NAME: &'static str = "thing";

    fn id(&self) -> &String {
        &self.id
    }
}

impl FamilyDefinition for Thing {
    fn base(&self) -> Option<&String> {
        self.base.as_ref()
    }

    fn family(&self) -> bool {
        self.r#abstract
    }

    fn inherit(&self, base: &Self) -> Self {
        Self {
            weight: self.weight.or(base.weight),
            ..self.clone()
        }
    }
}

/// An app loading `tests/assets/data`.
fn loading() -> TestApp {
    TestApp::builder()
        .plugins((
            AssetPlugin {
                file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets").to_owned(),
                ..default()
            },
            StatesPlugin,
            DataRegistryPlugin::new("data").with_options(
                ron::Options::default()
                    .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME),
            ),
        ))
        .with(|app| {
            app.add_family_definitions::<Thing>();
        })
        .build()
}

#[bevy_harness::test(setup = loading(), timeout = 10)]
fn definitions_load_index_and_come_whole(app: &mut TestApp) {
    app.until(|app| *app.resource::<State<DataState>>().get() == DataState::Loaded)
        .describe("the definitions load")
        .within_secs(10.0);
    app.run_system(|things: Defs<Thing>| {
        let weight = |id: &str| things.get(&id.to_owned()).and_then(|thing| thing.weight);
        assert_eq!(weight("pebble"), Some(1));
        assert_eq!(weight("boulder"), Some(5), "from its family");
        assert_eq!(weight("flint"), Some(2), "its own, from a group's folder");
        assert!(
            things.get(&"rocks".to_owned()).is_none(),
            "a family isn't read"
        );
        let mut ids: Vec<&String> = things.iter().map(|(id, _)| id).collect();
        ids.sort();
        assert_eq!(ids, ["boulder", "flint", "gravel", "pebble"]);
        assert_eq!(weight("gravel"), Some(1), "in a folder named as a file");
    });
}
