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
    (id: "reagent", family: true, price: Some(5)),
    (id: "silk", price: Some(3)),
    (id: "venom_sac", base: Some("reagent")),
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
  `resolve::Resolve`). `app.register_definition::<T>()` adds it.
- **Claims**: every data file is exactly one kind's. A file no kind
  claims, or two kinds do, is an error naming the rules; never settled by
  precedence.
- **Loading**: the data folder is loaded on startup, then every kind is
  indexed by id; ids used twice are errors. `DataState` moves to `Loaded`
  once it is, or to `Failed`, every error logged and kept in `DataErrors`,
  and the app told to exit (unless `exit_on_failure` is off). A missing
  data folder is an error.
- **Families**: a kind with a `#[definition(base)]` field can build on
  families: a definition takes from its base what it doesn't say
  (`Inherit`). One whose `#[definition(family)]` field is `true` is a
  family itself: only built on, never read, nor named by a reference. A
  missing base or a cycle is an error.
- **Ids**: `Id<T>`, the id of a definition of kind `T`: written as a name
  in the data (`"pelt"`), held as a 64-bit hash, `Copy`, and typed by its
  kind. As a kind's own id, and as a reference to another definition
  (`drops: Vec<Id<Item>>`), every one but the id, the base and
  `#[definition(unchecked)]` fields checked to name a definition `get`
  finds, its field named in the error (`finds.tag`). Checked are what
  `get` gives and the families as written; one naming a definition whose
  base doesn't resolve isn't reported again (that's an error already), and
  `get` may not find it after a reload. It shows as its hash; `Definitions::name`
  gives the name it was read from. Two names of a kind with one hash are
  an error. Any other id type works too (`String`), unchecked.
- **Qualifying**: `DataRegistryPlugin::qualify`, a `Qualify` rule (or a
  closure), turns each name read from a data file into the id's full name
  before it's hashed: `stone` into `base:stone`, so both are one id. It's
  told the file's path and the kind, and may reject a name, failing its
  file. Ids made in code aren't qualified: write them in full.
- **Reading**: `Definitions<T>`, a system param, as `Assets<T>` is: `get`,
  `contains`, `iter`, `name`, every definition whole.
- **Validating**: references are checked by the crate; your other checks
  across kinds run in `DataSystems::Validate`, pushing what's wrong to
  `DataErrors`. A type of your own holding `Id`s derives `References` to
  have them checked; a field marked `#[definition(unchecked)]` isn't.
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
    id: Id<Monster>,
    drops: Vec<Id<Item>>, // each checked to be an item
    health: u32,
}

#[derive(Definition, Deserialize, Clone)]
#[definition(file = "items", extension = "items")]
struct Item {
    id: Id<Item>,
    #[definition(base)]
    base: Option<Id<Item>>,
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

/// A check of the app's own: references are checked already.
fn alive(monsters: Definitions<Monster>, mut errors: ResMut<DataErrors>) {
    for (id, _) in monsters.iter().filter(|(_, monster)| monster.health == 0) {
        let name = monsters.name(id).unwrap_or_default();
        errors.push(format!("monster `{name}` has no health"));
    }
}

App::new()
    .add_plugins((DefaultPlugins, DataRegistryPlugin::default()))
    .register_definition::<Monster>()
    .register_definition::<Item>()
    .add_systems(Update, alive.in_set(DataSystems::Validate))
    .run();
```

`DataRegistryPlugin`'s fields set the folder (`path`, any asset source)
and the formats files are written in (`formats`, `format::Ron` by default), each
picked by a file's extension.

The data folder is loaded with Bevy's `load_folder`, and shares its
limits:

- Every file in it must be one some asset loader reads: Bevy fails the
  folder over one none does (`notes.txt`). One another loader reads is
  skipped, with a warning; so is one whose `.meta` names a loader the app
  doesn't have, without one.
- With `dev`, a file added with a malformed `.meta` fails the folder's
  reload until the folder changes again, or the file itself does: Bevy
  may not reload a file when only its `.meta` changes.
- On first load, a file that fails to load fails the folder, before the
  files after it are tried: broken files are reported one at a time.
- With `dev`, a folder deleted or moved out while the app runs isn't
  noticed: its definitions stay until a restart. Deleting its files first
  is.
- No folder in it can link back to one it's in, or loading never ends.

## Formats

A format is a `format::DataFormat`: its extensions, and reading a file's bytes
with the `FileSeed` it's given, which knows the kind and whether the file
holds one or a list. Any serde format whose deserializer supports
`deserialize_any` (JSON, TOML, YAML; not Postcard or Bincode) fits in a
few lines:

```rust,ignore
use bevy_data_registry::format::{DataFormat, FileContents, FileSeed, Ron};
use serde::de::DeserializeSeed;

struct Json;

impl DataFormat for Json {
    fn extensions(&self) -> &[&'static str] {
        &["json"]
    }

    fn read(&self, bytes: &[u8], seed: FileSeed<'_>) -> Result<FileContents, BevyError> {
        let mut json = serde_json::Deserializer::from_slice(bytes);
        let contents = seed.deserialize(&mut json)?;
        json.end()?; // nothing after it
        Ok(contents)
    }
}

app.add_plugins(DataRegistryPlugin {
    formats: vec![Arc::new(Ron::default()), Arc::new(Json)],
    ..default()
});
```

The rules claim files whatever their format: `file = "items"` claims
`items.ron` and `items.json` alike. A TOML file is a table, so holds one
definition: a list (`[[items]]`) is a table holding an array, which a TOML
format would have to present as the file itself to read as a list.

## Examples

Each reads its own folder under `assets/`:

- `basic`: kinds claiming files by name and extension, looked up by id.
- `families`: a kind claiming a folder; definitions built on families,
  made whole.
- `validation`: a reference to nothing, and a check of the example's own,
  failing loading, as its data is made to.
- `hot_reload` (`--features dev`): edit the data while it runs.

```sh
cargo run --example basic
cargo run --example hot_reload --features dev
```

## Layout

- `definition`: the `Definition` and `Inherit` traits; `DefinitionInfo`,
  what the crate knows of a kind, and `Families`, how its definitions
  build on others.
- `id`: `Id`; the names ids are read from, kept per file.
- `references`: `References` and `Referenced`, finding the `Id`s in a
  definition.
- `resolve` (public): `Resolve`, which files are a kind's; `FileName`,
  `Folder` and `Extension`.
- `format` (public): `DataFormat`, what files are written in; `Ron`;
  `FileSeed`, `FileContents`, `DataFile` and `DataFileError`, re-exported
  from `file`.
- `app`: `DataApp`, adding kinds to an app.
- `file`: `DataFile`, a file's definitions; the kinds and the files each
  claims; the loader, reading a file as its kind in its format.
- `registry`: `Registry`, each definition by id as last indexed, its
  families made whole; `Definitions`, reading them.
- `pipeline`: `DataState`, `DataSystems`, `DataErrors`, and the steps of
  each frame: `load` (the folder, until every file is settled), `index`,
  `references` (checking every `Id`), `finish` (the first load's end, and
  each reload's).
