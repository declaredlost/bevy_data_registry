//! The whole pipeline, as an app runs it: loading the data folder, each
//! file read as the kind that claims it in its format, indexing, families
//! made whole, validating, and `Definitions` reading them.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::asset::{AssetPlugin, UntypedAssetLoadFailedEvent};
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
        let mut json = serde_json::Deserializer::from_slice(bytes);
        let contents = seed.deserialize(&mut json)?;
        json.end()?;
        Ok(contents)
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
    app(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets").to_owned(),
        folder,
        false,
    )
}

/// An app loading `<assets>/<folder>`, watching it for changes if `watch`.
fn app(assets: String, folder: &'static str, watch: bool) -> TestApp {
    TestApp::builder()
        .plugins((
            AssetPlugin {
                file_path: assets,
                watch_for_changes_override: Some(watch),
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
        .watch::<UntypedAssetLoadFailedEvent>()
        .with(|app| {
            app.register_definition::<Thing>()
                .register_definition::<Tag>()
                .register_definition::<Thing>()
                .add_systems(Update, tags_exist.in_set(DataSystems::Validate));
        })
        .build()
}

/// Ticks until `holds`, for up to 10s of real time: as long as loading,
/// or the file watcher, takes, however fast ticks run.
fn wait(app: &mut TestApp, what: &str, mut holds: impl FnMut(&mut TestApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !holds(app) {
        assert!(Instant::now() < deadline, "{what}: not within 10s");
        app.ticks(1);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn state(app: &TestApp) -> DataState {
    *app.resource::<State<DataState>>().get()
}

#[bevy_harness::test(setup = loading("data"), timeout = 10)]
fn definitions_load_index_and_come_whole(app: &mut TestApp) {
    wait(app, "the definitions load", |app| {
        state(app) != DataState::Loading
    });
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
    wait(app, "loading ends", |app| state(app) != DataState::Loading);
    assert_eq!(state(app), DataState::Failed);
    app.assert_wrote::<UntypedAssetLoadFailedEvent>(|failure| {
        failure.path.path().ends_with("rocks.ron")
            && failure
                .error
                .to_string()
                .contains("no kind claims this file")
    });
}

#[bevy_harness::test(setup = loading("nowhere://data"), timeout = 10)]
fn an_unregistered_source_fails_loading(app: &mut TestApp) {
    wait(app, "loading ends", |app| state(app) != DataState::Loading);
    assert_eq!(state(app), DataState::Failed);
}

#[bevy_harness::test(setup = loading("data"), timeout = 10)]
fn loading_is_not_taken_for_a_reload(app: &mut TestApp) {
    wait(app, "the definitions load", |app| {
        state(app) != DataState::Loading
    });
    assert_eq!(state(app), DataState::Loaded);
    app.ticks(5);
    app.assert_wrote_none::<DataReloaded>(|_| true);
}

#[bevy_harness::test(setup = loading("data"), timeout = 10)]
fn a_file_outside_the_data_folder_is_no_reload(app: &mut TestApp) {
    wait(app, "the definitions load", |app| {
        state(app) != DataState::Loading
    });
    assert_eq!(state(app), DataState::Loaded);
    // Read by the registry's loader, as a `.ron`, and failing; but not data.
    let stray = app
        .resource::<AssetServer>()
        .load::<DataFile>("broken/rocks.ron");
    wait(app, "the stray file fails", |app| {
        app.resource::<AssetServer>().load_state(&stray).is_failed()
    });
    app.ticks(5);
    app.assert_wrote_none::<DataReloaded>(|_| true);
}

/// Hot reload: data files written while the app runs, in a temporary
/// folder of the test's own.
/// A test's own assets folder: deleted as the app is dropped.
#[derive(Resource)]
struct Scratch(tempfile::TempDir);

/// An app loading a temporary assets folder whose `data/tags.ron` holds the
/// tag `old`, and what `fill` puts in its `data/`; watching it if `watch`.
fn scratch(watch: bool, fill: impl FnOnce(&std::path::Path)) -> TestApp {
    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::write(data.join("tags.ron"), r#"[(id: "old")]"#).unwrap();
    fill(&data);
    // Bevy's watcher matches paths by prefix: macOS's `/var` links to
    // `/private/var`.
    let root = scratch.path().canonicalize().unwrap();
    let mut app = app(root.to_str().unwrap().to_owned(), "data", watch);
    app.world_mut().insert_resource(Scratch(scratch));
    app
}

#[cfg(unix)]
fn a_broken_link() -> TestApp {
    scratch(false, |data| {
        std::os::unix::fs::symlink(data.join("nowhere.ron"), data.join("gone.ron")).unwrap();
    })
}

#[cfg(unix)]
#[bevy_harness::test(setup = a_broken_link(), timeout = 10)]
fn a_file_listed_but_never_there_fails_loading(app: &mut TestApp) {
    wait(app, "loading ends", |app| state(app) != DataState::Loading);
    assert_eq!(state(app), DataState::Failed);
}

#[cfg(feature = "dev")]
mod hot_reload {
    use std::path::PathBuf;

    use bevy::asset::{LoadState, LoadedFolder};

    use super::*;

    /// `<the test's assets>/data/<file>`.
    fn data(app: &TestApp, file: &str) -> PathBuf {
        app.resource::<Scratch>().0.path().join("data").join(file)
    }

    /// An app watching a temporary assets folder whose `data/tags.ron` holds
    /// the tag `old`.
    fn watching() -> TestApp {
        scratch(true, |_| {})
    }

    fn has(app: &mut TestApp, id: &'static str) -> bool {
        app.run_system(move |tags: Definitions<Tag>| tags.contains(id))
    }

    fn folder_failed(app: &TestApp) -> bool {
        let server = app.resource::<AssetServer>();
        server
            .get_handle::<LoadedFolder>("data")
            .is_some_and(|folder| matches!(server.load_state(&folder), LoadState::Failed(_)))
    }

    #[bevy_harness::test(setup = watching(), timeout = 30)]
    fn a_file_added_broken_loads_once_fixed(app: &mut TestApp) {
        wait(app, "the definitions load", |app| {
            state(app) != DataState::Loading
        });
        assert_eq!(state(app), DataState::Loaded);
        app.read::<DataReloaded>();
        // Saved half-written, as an editor would: the folder's reload fails.
        let new = data(app, "new.tags.ron");
        std::fs::write(&new, "[(id: ").unwrap();
        let mut reported = false;
        wait(app, "the folder's failed reload is reported", |app| {
            reported |= !app.read::<DataReloaded>().is_empty();
            reported && folder_failed(app)
        });
        assert!(has(app, "old"), "what was loaded is kept");
        // Fixed in place.
        std::fs::write(&new, r#"[(id: "new")]"#).unwrap();
        wait(app, "the fixed file loads", |app| has(app, "new"));
        assert!(has(app, "old"));
        assert_eq!(state(app), DataState::Loaded);
    }
}
