<div align="center">

# bevy_data_registry

**Data definitions for Bevy: files claimed by kind, indexed by id, validated and hot-reloaded.**

[![Build](https://img.shields.io/github/actions/workflow/status/declaredlost/bevy_data_registry/build.yml?branch=main&label=build&style=flat-square)](https://github.com/declaredlost/bevy_data_registry/actions/workflows/build.yml)
[![Security](https://img.shields.io/github/actions/workflow/status/declaredlost/bevy_data_registry/security.yml?branch=main&label=security&style=flat-square)](https://github.com/declaredlost/bevy_data_registry/actions/workflows/security.yml)

</div>

```ron
// assets/data/tags.ron
[
    (id: "venomous"),
]

// assets/data/enemies/spider/monsters.ron
[
    (id: "spider", tags: ["venomous"]),
]
```

```rust,ignore
#[derive(Definition, Deserialize)]
#[definition(file = "tags")]
struct Tag { id: Id<Tag> }

#[derive(Definition, Deserialize)]
#[definition(file = "monsters", folder = "bestiary")]
struct Monster { id: Id<Monster>, tags: Vec<Id<Tag>> } // each checked to be a tag

const SPIDER: Id<Monster> = Id::from_name("spider");

app.add_plugins(DataRegistryPlugin::default())
    .register_definition::<Tag>()
    .register_definition::<Monster>();

fn spawn(monsters: Definitions<Monster>) {
    let spider = monsters.get(&SPIDER);
}
```

> [!WARNING]
> **Early development.** The API may change at any time.

## Crates

| Crate | Description |
|---|---|
| [`bevy_data_registry`](crates/bevy_data_registry) | Definitions read from the files each kind claims, indexed by id, validated and hot-reloaded |
| [`bevy_data_registry_macros`](crates/bevy_data_registry_macros) | `#[derive(Definition)]`, re-exported by `bevy_data_registry` |

## Requirements

- **Rust + Cargo**: 1.99+, Edition 2024
- **Bevy**: 0.20

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines, and
[CHANGELOG.md](CHANGELOG.md) for release notes.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
