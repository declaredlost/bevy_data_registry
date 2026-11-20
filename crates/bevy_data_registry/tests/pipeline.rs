//! The whole pipeline, as an app runs it: loading the data folder, each
//! file read as the kind that claims it in its format, indexing, families
//! made whole, validating, and `Definitions` reading them.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::asset::io::{AssetSource, AssetSourceBuilder};
use bevy::asset::{AssetMode, AssetPlugin, UntypedAssetLoadFailedEvent};
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
    id: Id<Thing>,
    #[definition(base)]
    #[serde(default)]
    base: Option<Id<Thing>>,
    #[definition(family)]
    #[serde(default)]
    family: bool,
    #[serde(default)]
    weight: Option<u32>,
    /// Checked to be tags by the crate.
    #[serde(default)]
    tags: Vec<Id<Tag>>,
    /// Checked through `Find`.
    #[serde(default)]
    finds: Vec<Find>,
    /// Another thing it's like: checked to be one `get` finds.
    #[serde(default)]
    like: Option<Id<Thing>>,
}

/// What's found in a thing: a type of the app's own holding an `Id`.
#[derive(References, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Find {
    tag: Id<Tag>,
    /// Never checked: may name no tag.
    #[definition(unchecked)]
    #[serde(default)]
    rumour: Option<Id<Tag>>,
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
    id: Id<Tag>,
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

/// Every thing weighs something: a check of the app's own.
fn weighed(things: Definitions<Thing>, mut errors: ResMut<DataErrors>) {
    for (id, _) in things.iter().filter(|(_, thing)| thing.weight.is_none()) {
        errors.push(format!("thing `{id}` weighs nothing"));
    }
}

/// An app loading `tests/assets/<folder>`.
fn loading(folder: &'static str) -> TestApp {
    app(ASSETS.to_owned(), plugin(folder), false)
}

/// The test assets.
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets");

/// The plugin loading `<assets>/<folder>`, in RON and JSON.
fn plugin(folder: &'static str) -> DataRegistryPlugin {
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
        ..default()
    }
}

/// An app loading `<assets>` with `plugin`, watching for changes if
/// `watch`.
fn app(assets: String, plugin: DataRegistryPlugin, watch: bool) -> TestApp {
    TestApp::builder()
        .plugins((
            AssetPlugin {
                file_path: assets,
                watch_for_changes_override: Some(watch),
                ..default()
            },
            StatesPlugin,
            plugin,
        ))
        .watch::<DataReloaded>()
        .watch::<AppExit>()
        .watch::<UntypedAssetLoadFailedEvent>()
        .with(|app| {
            app.register_definition::<Thing>()
                .register_definition::<Tag>()
                .register_definition::<Thing>()
                .add_systems(Update, weighed.in_set(DataSystems::Validate));
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

/// Waits for loading to end, and asserts it failed with one error, saying
/// all of `says`.
fn fails_with(app: &mut TestApp, says: &[&str]) {
    wait(app, "loading ends", |app| state(app) != DataState::Loading);
    assert_eq!(state(app), DataState::Failed);
    let errors: Vec<&str> = app.resource::<DataErrors>().iter().collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    for part in says {
        assert!(errors[0].contains(part), "{part:?} in {errors:?}");
    }
}

#[bevy_harness::test(setup = loading("data"), timeout = 10)]
fn definitions_load_index_and_come_whole(app: &mut TestApp) {
    wait(app, "the definitions load", |app| {
        state(app) != DataState::Loading
    });
    assert_eq!(state(app), DataState::Loaded);
    app.run_system(|things: Definitions<Thing>, tags: Definitions<Tag>| {
        let weight = |id: &str| {
            things
                .get(&Id::from_name(id))
                .and_then(|thing| thing.weight)
        };
        assert_eq!(weight("pebble"), Some(1));
        assert_eq!(weight("boulder"), Some(5), "from its family");
        let boulder = things.get(&Id::from_name("boulder")).expect("loaded");
        assert_eq!(boulder.like, Some(Id::from_name("pebble")));
        assert_eq!(weight("flint"), Some(2), "its own, alone in its file");
        assert_eq!(weight("slate"), Some(5), "from a file named for it");
        assert_eq!(weight("gravel"), Some(1), "in a folder named as a file");
        assert!(things.get(&ROCKS).is_none(), "a family isn't read");
        assert!(!things.contains(&ROCKS));
        let mut ids: Vec<&str> = things.ids().filter_map(|id| things.name(id)).collect();
        ids.sort_unstable();
        assert_eq!(ids, ["boulder", "flint", "gravel", "pebble", "slate"]);
        assert_eq!(things.len(), 5);
        assert_eq!(
            things
                .path(&Id::from_name("flint"))
                .map(ToString::to_string)
                .as_deref(),
            Some("data/stones/things.ron")
        );
        assert_eq!(tags.len(), 3);
        let tag = |name| tags.contains(&Id::from_name(name));
        assert!(tag("heavy") && tag("sharp"));
        let slate = things.get(&Id::from_name("slate")).expect("loaded");
        assert_eq!(
            slate.tags,
            [Id::from_name("heavy"), Id::from_name("smooth")]
        );
        assert_eq!(tags.name(&slate.tags[0]), Some("heavy"), "named as read");
        assert_eq!(slate.finds[0].tag, Id::from_name("sharp"));
        assert_eq!(
            slate.finds[0].rumour,
            Some(Id::from_name("glowing")),
            "unchecked: no tag, yet loaded"
        );
        assert!(tag("smooth"), "from JSON, by its extension");
    });
}

/// A family: never read.
const ROCKS: Id<Thing> = Id::from_name("rocks");

#[bevy_harness::test(setup = loading("dangling"), timeout = 10)]
fn a_reference_to_what_isnt_there_fails_loading(app: &mut TestApp) {
    fails_with(
        app,
        &["Thing `pebble`", "`tags`", "Tag `nope`, which isn't there"],
    );
    app.ticks(1);
    app.assert_wrote::<AppExit>(AppExit::is_error);
}

/// Loading `dangling`, not exiting when it fails.
fn not_exiting() -> TestApp {
    let plugin = DataRegistryPlugin {
        exit_on_failure: false,
        ..plugin("dangling")
    };
    app(ASSETS.to_owned(), plugin, false)
}

#[bevy_harness::test(setup = not_exiting(), timeout = 10)]
fn failing_leaves_exiting_to_the_app_if_told_to(app: &mut TestApp) {
    fails_with(app, &["Tag `nope`"]);
    app.ticks(3);
    app.assert_wrote_none::<AppExit>(|_| true);
}

#[bevy_harness::test(setup = loading("like_a_family"), timeout = 10)]
fn a_reference_to_a_family_fails_loading(app: &mut TestApp) {
    fails_with(
        app,
        &["Thing `pebble`", "`like`", "Thing `rocks`, a family"],
    );
}

#[bevy_harness::test(setup = loading("dangling_find"), timeout = 10)]
fn a_reference_in_a_type_of_the_apps_own_is_checked(app: &mut TestApp) {
    fails_with(
        app,
        &[
            "Thing `pebble`",
            "`finds.tag`",
            "Tag `nope`, which isn't there",
        ],
    );
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
    fails_with(app, &["asset source `nowhere` isn't registered"]);
}

/// Loading `raw://data` where assets are processed, `raw` having no
/// processed reader.
fn processed() -> TestApp {
    TestApp::builder()
        .plugins((
            |app: &mut App| {
                app.register_asset_source(
                    "raw",
                    AssetSourceBuilder::new(AssetSource::get_default_reader(ASSETS.to_owned())),
                );
            },
            AssetPlugin {
                file_path: ASSETS.to_owned(),
                mode: AssetMode::Processed,
                use_asset_processor_override: Some(false),
                ..default()
            },
            StatesPlugin,
            plugin("raw://data"),
        ))
        .build()
}

#[bevy_harness::test(setup = processed(), timeout = 10)]
fn a_source_with_no_processed_reader_fails_loading_where_processed(app: &mut TestApp) {
    fails_with(app, &["asset source `raw` has no processed reader"]);
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
#[cfg_attr(
    not(feature = "dev"),
    expect(dead_code, reason = "kept only to be deleted when the test ends")
)]
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
    let mut app = app(root.to_str().unwrap().to_owned(), plugin("data"), watch);
    app.world_mut().insert_resource(Scratch(scratch));
    app
}

fn a_note() -> TestApp {
    scratch(false, |data| {
        std::fs::write(data.join("notes.txt"), "not data").unwrap();
    })
}

/// Bevy's `load_folder` fails over a file no loader reads: see the README.
#[bevy_harness::test(setup = a_note(), timeout = 10)]
fn a_file_no_loader_reads_fails_loading(app: &mut TestApp) {
    fails_with(app, &["notes.txt", "no asset loader reads it"]);
}

#[bevy_harness::test(setup = loading("nowhere"), timeout = 10)]
fn a_missing_data_folder_fails_loading(app: &mut TestApp) {
    fails_with(app, &["data folder nowhere"]);
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
    fails_with(app, &["gone.ron"]);
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
        app.run_system(move |tags: Definitions<Tag>| tags.contains(&Id::from_name(id)))
    }

    fn folder_failed(app: &TestApp) -> bool {
        let server = app.resource::<AssetServer>();
        server
            .get_handle::<LoadedFolder>("data")
            .is_some_and(|folder| matches!(server.load_state(&folder), LoadState::Failed(_)))
    }

    /// Whether [`DataReloaded`] was sent in a frame before validating began
    /// in it: it's to come once indexing and validating are done.
    #[derive(Resource, Default)]
    struct SentEarly(bool);

    fn sent_early(reloaded: Res<Messages<DataReloaded>>, mut early: ResMut<SentEarly>) {
        early.0 |= reloaded.iter_current_update_messages().next().is_some();
    }

    /// Whether the tag `new` was there, for each [`DataReloaded`] read in
    /// `PostUpdate`, as the docs say to.
    #[derive(Resource, Default)]
    struct NewWhenReloaded(Vec<bool>);

    fn new_when_reloaded(
        mut reloaded: MessageReader<DataReloaded>,
        tags: Definitions<Tag>,
        mut seen: ResMut<NewWhenReloaded>,
    ) {
        for _ in reloaded.read() {
            seen.0.push(tags.contains(&Id::from_name("new")));
        }
    }

    #[bevy_harness::test(setup = watching(), timeout = 30)]
    fn a_file_added_broken_loads_once_fixed(app: &mut TestApp) {
        app.app_mut()
            .init_resource::<SentEarly>()
            .init_resource::<NewWhenReloaded>()
            .add_systems(Update, sent_early.in_set(DataSystems::Validate))
            .add_systems(PostUpdate, new_when_reloaded);
        wait(app, "the definitions load", |app| {
            state(app) != DataState::Loading
        });
        assert_eq!(state(app), DataState::Loaded);
        app.read::<DataReloaded>();
        // Saved half-written, as an editor would: the folder's reload fails.
        let new = data(app, "new.tags.ron");
        std::fs::write(&new, "[(id: ").unwrap();
        wait(app, "the folder's failed reload is reported", |app| {
            let reported = !app.read::<DataReloaded>().is_empty();
            let failed = folder_failed(app);
            assert!(!reported || failed, "reported before the folder failed");
            reported
        });
        assert!(has(app, "old"), "what was loaded is kept");
        // Fixed in place.
        std::fs::write(&new, r#"[(id: "new")]"#).unwrap();
        wait(app, "the fixed file loads, and that's reported", |app| {
            !app.read::<DataReloaded>().is_empty() && has(app, "new")
        });
        assert!(has(app, "old"));
        assert_eq!(state(app), DataState::Loaded);
        assert_eq!(
            app.resource::<NewWhenReloaded>().0.last(),
            Some(&true),
            "there as it's reported"
        );
        // Deleted: what it held goes.
        std::fs::remove_file(&new).unwrap();
        wait(app, "the deleted file's tag goes", |app| !has(app, "new"));
        assert!(has(app, "old"));
        assert!(!app.resource::<SentEarly>().0, "reported before validating");
    }
}
