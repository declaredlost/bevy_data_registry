//! The whole pipeline, as an app runs it: loading the data folder, each
//! file read as the kind that claims it in its format, indexing, families
//! made whole, validating, and `Definitions` reading them.

use std::sync::Arc;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_data_registry::format::{DataFile, DataFormat, FileContents, FileSeed, Ron};
use bevy_data_registry::prelude::*;
use bevy_harness::TestApp;
use serde::Deserialize;
use serde::de::DeserializeSeed;

/// Under `things/`, or in files named `things`.
#[derive(Definition, Debug, Clone, Deserialize)]
#[definition(folder = "things", file = "things")]
#[serde(deny_unknown_fields)]
struct Thing {
    id: String,
    #[definition(base)]
    #[serde(default)]
    base: Option<String>,
    #[definition(family)]
    #[serde(default)]
    family: bool,
    #[serde(default)]
    weight: Option<u32>,
    #[serde(default)]
    tags: Vec<String>,
}

impl Inherit for Thing {
    fn inherit(&mut self, base: &Self) {
        self.weight = self.weight.or(base.weight);
    }
}

/// In files named `tags`, or ending `.tags.<format>`.
#[derive(Definition, Debug, Deserialize)]
#[definition(file = "tags", extension = "tags")]
#[serde(deny_unknown_fields)]
struct Tag {
    id: String,
}

/// A second format, written outside the crate.
struct Json;

impl DataFormat for Json {
    fn extensions(&self) -> &[&'static str] {
        &["json"]
    }

    fn read(&self, bytes: &[u8], seed: FileSeed<'_>) -> Result<FileContents, BevyError> {
        Ok(seed.deserialize(&mut serde_json::Deserializer::from_slice(bytes))?)
    }
}

/// Every thing's tags are tags.
fn tags_exist(things: Definitions<Thing>, tags: Definitions<Tag>, mut errors: ResMut<DataErrors>) {
    for (id, thing) in &things {
        for tag in thing.tags.iter().filter(|tag| !tags.contains(*tag)) {
            errors.push(format!("thing `{id}` has tag `{tag}`, which isn't there"));
        }
    }
}

/// An app loading `tests/assets/<folder>`.
fn loading(folder: &'static str) -> TestApp {
    TestApp::builder()
        .plugins((
            AssetPlugin {
                file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets").to_owned(),
                ..default()
            },
            StatesPlugin,
            DataRegistryPlugin {
                path: folder.into(),
                formats: vec![
                    Arc::new(Ron {
                        options: ron::Options::default()
                            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME),
                        ..default()
                    }),
                    Arc::new(Json),
                ],
            },
        ))
        .watch::<DataReloaded>()
        .with(|app| {
            app.register_definition::<Thing>()
                .register_definition::<Tag>()
                .register_definition::<Thing>()
                .add_systems(Update, tags_exist.in_set(DataSystems::Validate));
        })
        .build()
}

fn state(app: &TestApp) -> DataState {
    *app.resource::<State<DataState>>().get()
}

#[bevy_harness::test(setup = loading("data"), timeout = 10)]
fn definitions_load_index_and_come_whole(app: &mut TestApp) {
    app.until(|app| state(app) != DataState::Loading)
        .describe("the definitions load")
        .within_secs(10.0);
    assert_eq!(state(app), DataState::Loaded);
    app.run_system(|things: Definitions<Thing>, tags: Definitions<Tag>| {
        let weight = |id: &str| things.get(id).and_then(|thing| thing.weight);
        assert_eq!(weight("pebble"), Some(1));
        assert_eq!(weight("boulder"), Some(5), "from its family");
        assert_eq!(weight("flint"), Some(2), "its own, alone in its file");
        assert_eq!(weight("slate"), Some(5), "from a file named for it");
        assert_eq!(weight("gravel"), Some(1), "in a folder named as a file");
        assert!(things.get("rocks").is_none(), "a family isn't read");
        assert!(!things.contains("rocks"));
        let mut ids: Vec<&String> = things.ids().collect();
        ids.sort();
        assert_eq!(ids, ["boulder", "flint", "gravel", "pebble", "slate"]);
        assert_eq!(things.len(), 5);
        assert_eq!(
            things.path("flint").map(ToString::to_string).as_deref(),
            Some("data/stones/things.ron")
        );
        assert_eq!(tags.len(), 3);
        assert!(tags.contains("heavy") && tags.contains("sharp"));
        assert!(tags.contains("smooth"), "from JSON, by its extension");
    });
}

#[bevy_harness::test(setup = loading("broken"), timeout = 10)]
fn a_file_no_kind_claims_fails_loading(app: &mut TestApp) {
    app.until(|app| state(app) != DataState::Loading)
        .describe("loading ends")
        .within_secs(10.0);
    assert_eq!(state(app), DataState::Failed);
}

#[bevy_harness::test(setup = loading("data"), timeout = 10)]
fn loading_is_not_taken_for_a_reload(app: &mut TestApp) {
    app.until(|app| state(app) != DataState::Loading)
        .describe("the definitions load")
        .within_secs(10.0);
    app.ticks(5);
    app.assert_wrote_none::<DataReloaded>(|_| true);
}

#[bevy_harness::test(setup = loading("data"), timeout = 10)]
fn a_file_outside_the_data_folder_is_no_reload(app: &mut TestApp) {
    app.until(|app| state(app) != DataState::Loading)
        .describe("the definitions load")
        .within_secs(10.0);
    // Read by the registry's loader, as a `.ron`, and failing; but not data.
    let stray = app
        .resource::<AssetServer>()
        .load::<DataFile>("broken/rocks.ron");
    app.until(|app| app.resource::<AssetServer>().load_state(&stray).is_failed())
        .describe("the stray file fails")
        .within_secs(10.0);
    app.ticks(5);
    app.assert_wrote_none::<DataReloaded>(|_| true);
}
