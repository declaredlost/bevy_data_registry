# bevy_data_registry_macros

The derive macro of [`bevy_data_registry`](../bevy_data_registry): use it
through that crate, which re-exports it, rather than depending on this one.

## `#[derive(Definition)]`

Makes a struct a kind of definition: implements `Definition` for it, its
name (in messages) the struct's.

```rust,ignore
use bevy_data_registry::prelude::*;
use serde::Deserialize;

#[derive(Definition, Deserialize, Clone)]
#[definition(file = "items", folder = "items")]
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
```

On the struct, which must have named fields:

- Which data files are its, at least one, any number of each; it claims
  a file any of them claims:
  - `#[definition(file = "items")]`: every file named `items`, in any
    folder: `items.ron`, `enemies/spider/items.json`.
  - `#[definition(folder = "weapons")]`: every file under `weapons/`, at
    any depth, from the data folder's top. Can be several deep:
    `affixes/prefixes`.
  - `#[definition(extension = "items")]`: every file ending
    `.items.<format>`: `swords.items.ron`.
  - `#[definition(resolve = ..)]`: any value of a type implementing
    `resolve::Resolve`: `resolve = TopLevel("loot")`.
- `#[definition(crate = "..")]`: the path to `bevy_data_registry`, where
  it's renamed or re-exported (`crate = "engine::data"`).
  `::bevy_data_registry` if left out.

On its fields, each at most once:

- `#[definition(id)]`: what tells its definitions apart. The field named
  `id` if none is marked.
- `#[definition(base)]`: an `Option` of the id, what a definition builds
  on. Makes it a kind with families: the struct must then be `Clone` and
  implement `Inherit`, which fills a clone of the definition in from its
  base.
- `#[definition(family)]`: a `bool`, whether a definition is a family,
  only built on and never read. Needs a `base` field.

## Layout

- `lib`: the macro's entry point.
- `definition`: `#[derive(Definition)]`: its attributes, and the impl it
  writes.
