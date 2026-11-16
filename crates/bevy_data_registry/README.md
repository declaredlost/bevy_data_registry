# bevy_data_registry

Data definitions for Bevy: plain Rust types, written as RON files, each
file one definition or a list of them. A file's kind is the first folder
it's in that's a kind's (`assets/data/items/swords.ron`, subfolders
included), or else its name (`assets/data/monsters/spiders/items.ron`): one
folder per kind, or a folder per group holding a file per kind, mixed
freely. A file of no kind is an error. See `kind_of`.

- **Loading**: every file of every kind is listed and loaded on startup,
  subfolders included, then indexed by each definition's id; ids used twice
  are errors. `DataState` moves to `Loaded` once all of them are, or to
  `Failed`, every error logged and the app told to exit.
- **Families**: a kind added with `add_family_definitions` (a
  `FamilyDefinition`) can build on families: abstract definitions, never
  read, whose whole a definition takes what it doesn't say from (its
  `inherit`). Made whole as they're indexed; a missing base or a cycle is
  an error.
- **Reading**: `Defs<T>`, a system param, gets definitions by id, whole.
- **Validating**: your checks across kinds run in `DataSystems::Validate`,
  pushing what's wrong to `DataErrors`.
- **Hot reload** (`dev`): a changed file is indexed and validated again,
  problems logged, and `DataReloaded` sent.

The definition types needn't depend on Bevy: implement `Definition` (and
`FamilyDefinition`) for them where the kinds are added.

```rust,ignore
#[derive(Deserialize)]
struct Tag { id: String }

impl Definition for Tag {
    const FOLDER: &'static str = "tags";
    const NAME: &'static str = "tag";
    type Id = String;
    fn id(&self) -> &String { &self.id }
}

#[derive(Clone, Deserialize)]
struct Item { id: String, base: Option<String>, r#abstract: bool, price: Option<u32> }

impl Definition for Item { /* as above, "items" */ }

impl FamilyDefinition for Item {
    fn base(&self) -> Option<&String> { self.base.as_ref() }
    fn family(&self) -> bool { self.r#abstract }
    fn inherit(&self, base: &Self) -> Self {
        Self { price: self.price.or(base.price), ..self.clone() }
    }
}

app.add_plugins(DataRegistryPlugin::new("data"))
    .add_definitions::<Tag>()
    .add_family_definitions::<Item>();
```

## Layout

- `definition`: the `Definition` and `FamilyDefinition` traits.
- `app`: `DataApp`, adding kinds to an app.
- `asset`: `DataAsset`, a file's definitions, and its RON loader.
- `files`: which kind a file holds (`kind_of`), and listing them.
- `registry`: `Registry`, where each definition is by id; its families
  made whole; `Defs`, reading them.
- `pipeline`: `DataState`, `DataSystems`, `DataErrors`, and the steps of
  each frame: `load` (listing, loading, waiting), `index`, `finish` (the
  first load's end, and each reload's).
