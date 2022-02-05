# [isixhosa.click](https://isixhosa.click)

An online glossary of IsiXhosa words, with English, Xhosa, and other relevant information provided along with a live
search and community editing features.

## Maintenance

To wipe the database, simply `rm -rf tantivy_data/` and `rm isixhosa_click.db`.

## Config

By default, it is configured as a development environment. See the `Config` struct in `main.rs` for more info. Under
Ubuntu and Oracle Linux, the config file will be stored in `~/.config/isixhosa_click/isixhosa_click.toml`.

## Building

You will need `wasm-bindgen-cli` and `wasm-opt`.