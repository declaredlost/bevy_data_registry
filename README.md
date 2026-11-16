<div align="center">

# bevy_data_registry

**Data definitions for Bevy: RON files loaded by kind, indexed by id, validated and hot-reloaded.**

[![Build](https://img.shields.io/github/actions/workflow/status/declaredlost/bevy_data_registry/build.yml?branch=main&label=build&style=flat-square)](https://github.com/declaredlost/bevy_data_registry/actions/workflows/build.yml)
[![Security](https://img.shields.io/github/actions/workflow/status/declaredlost/bevy_data_registry/security.yml?branch=main&label=security&style=flat-square)](https://github.com/declaredlost/bevy_data_registry/actions/workflows/security.yml)

</div>

```rust,ignore
#[derive(Deserialize)]
struct Tag { id: String }

impl Definition for Tag {
    const FOLDER: &'static str = "tags";
    const NAME: &'static str = "tag";
    type Id = String;
    fn id(&self) -> &String { &self.id }
}

app.add_plugins(DataRegistryPlugin::new("data"))
    .add_definitions::<Tag>();

fn burn(tags: Defs<Tag>) {
    let fire = tags.get(&"fire".to_owned());
}
```

> [!WARNING]
> **Early development.** The API may change at any time.

## Crates

| Crate | Description |
|---|---|
| [`bevy_data_registry`](crates/bevy_data_registry) | RON definition files loaded by kind, indexed by id, validated and hot-reloaded |

## Requirements

- **Rust + Cargo**: 1.99+, Edition 2024
- **Bevy**: 0.20

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines, and
[CHANGELOG.md](CHANGELOG.md) for release notes.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
