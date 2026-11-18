# bevy_data_registry

Data definitions for Bevy: plain Rust types, each read from the data
files it claims, by file name, folder or extension:

```text
assets/data/
  enemies/
    spider/{items,monsters,skills}.ron
    wolf/{items,monsters}.ron
  weapons/swords.ron, weapons/rare/relics.ron
```

```ron
// assets/data/enemies/spider/items.ron
[
    (id: "silk", price: 3),
    (id: "venom_sac", base: "reagent"),
]
```

A file holds one definition or a list of them, all of the kind that
claims it; every file under the data folder (`assets/data/` by default),
at any depth, is read.

- **Kinds**: `#[derive(Definition)]` on a struct makes it a kind, its id
  the field named `id` (or marked `#[definition(id)]`), its files those
  any of its rules claims: `#[definition(file = "items")]` (every
  `items.ron`), `folder = "weapons"` (every file under `weapons/`),
  `extension = "items"` (every `*.items.ron`), or `resolve = ..` (any
  `Resolve`). `app.register_definition::<T>()` adds it.
- **Claims**: every data file is exactly one kind's. A file no kind
  claims, or two kinds do, is an error naming the rules; never settled by
  precedence. Two kinds with the same rule panic as they're registered.
- **Loading**: the data folder is loaded on startup, then every kind is
  indexed by id; ids used twice are errors. `DataState` moves to `Loaded`
  once it is, or to `Failed`, every error logged and the app told to exit.
- **Families**: a kind with a `#[definition(base)]` field can build on
  families: a definition takes from its base what it doesn't say
  (`Inherit`). One whose `#[definition(family)]` field is `true` is a
  family itself: only built on, never read. A missing base or a cycle is
  an error.
- **Reading**: `Definitions<T>`, a system param, as `Assets<T>` is: `get`,
  `contains`, `iter`, every definition whole.
- **Validating**: your checks across kinds run in `DataSystems::Validate`,
  pushing what's wrong to `DataErrors`.
- **Hot reload** (`dev`): a file changed, added or removed is loaded again,
  every kind indexed and validated again, problems logged, and
  `DataReloaded` sent.

```rust,ignore
use bevy::prelude::*;
use bevy_data_registry::prelude::*;
use serde::Deserialize;

#[derive(Definition, Deserialize)]
#[definition(file = "monsters")]
struct Monster {
    id: String,
    drops: Vec<String>,
}

#[derive(Definition, Deserialize, Clone)]
#[definition(file = "items", extension = "items")]
struct Item {
    id: String,
    #[definition(base)]
    base: Option<String>,
    #[definition(family)]
    #[serde(default)]
    family: bool,
    price: Option<u32>,
}

impl Inherit for Item {
    fn inherit(&mut self, base: &Self) {
        self.price = self.price.or(base.price);
    }
}

fn drops_exist(
    monsters: Definitions<Monster>,
    items: Definitions<Item>,
    mut errors: ResMut<DataErrors>,
) {
    for (id, monster) in &monsters {
        for drop in monster.drops.iter().filter(|drop| !items.contains(*drop)) {
            errors.push(format!("monster `{id}` drops `{drop}`, which isn't there"));
        }
    }
}

App::new()
    .add_plugins((DefaultPlugins, DataRegistryPlugin::default()))
    .register_definition::<Monster>()
    .register_definition::<Item>()
    .add_systems(Update, drops_exist.in_set(DataSystems::Validate))
    .run();
```

`DataRegistryPlugin`'s fields set the folder (`path`, any asset source)
and the formats files are written in (`formats`, `Ron` by default), each
picked by a file's extension.

The data folder is loaded with Bevy's `load_folder`, and shares its
limits:

- Every file in it must be one some loader reads: Bevy fails the folder
  over one none does.
- On first load, a file that fails to load fails the folder, before the
  files after it are tried: broken files are reported one at a time.
- With `dev`, a folder deleted or moved out while the app runs isn't
  noticed: its definitions stay until a restart. Deleting its files first
  is.
- No folder in it can link back to one it's in, or loading never ends.

## Formats

A format is a `DataFormat`: its extensions, and reading a file's bytes
with the `FileSeed` it's given, which knows the kind and whether the file
holds one or a list. Any serde format fits in a few lines:

```rust,ignore
struct Json;

impl DataFormat for Json {
    fn extensions(&self) -> &[&'static str] {
        &["json"]
    }

    fn read(&self, bytes: &[u8], seed: FileSeed<'_>) -> Result<FileContents, BevyError> {
        Ok(seed.deserialize(&mut serde_json::Deserializer::from_slice(bytes))?)
    }
}

app.add_plugins(DataRegistryPlugin {
    formats: vec![Arc::new(Ron::default()), Arc::new(Json)],
    ..default()
});
```

The rules claim files whatever their format: `file = "items"` claims
`items.ron` and `items.json` alike. A TOML file is a table, so holds one
definition.

## Examples

Each reads its own folder under `assets/`:

- `basic`: kinds claiming files by name and extension, looked up by id.
- `families`: a kind claiming a folder; definitions built on families,
  made whole.
- `validation`: a check across kinds failing loading, as its data is made to.
- `hot_reload` (`--features dev`): edit the data while it runs.

```sh
cargo run --example basic
cargo run --example hot_reload --features dev
```

## Layout

- `definition`: the `Definition` and `Inherit` traits.
- `resolve`: `Resolve`, which files are a kind's; `FileName`, `Folder` and
  `Extension`.
- `format`: `DataFormat`, what files are written in; `Ron`.
- `app`: `DataApp`, adding kinds to an app.
- `file`: `DataFile`, a file's definitions; the kinds and the files each
  claims; the loader, reading a file as its kind in its format.
- `registry`: `Registry`, where each definition is by id, its families
  made whole; `Definitions`, reading them.
- `pipeline`: `DataState`, `DataSystems`, `DataErrors`, and the steps of
  each frame: `load` (the folder, until every file is settled), `index`,
  `finish` (the first load's end, and each reload's).
