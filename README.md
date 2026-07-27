# igdb-metadata-wasm-plugin

A `MetadataProviderPlugin` for [Concourse](https://github.com/smh0505/Concourse) implemented as
a WASM component, ported from that project's built-in `igdb.rs`. Fetches description/release
date/genres from [IGDB](https://www.igdb.com/) by title search - no cover/background art,
that's SteamGridDB's job (see `sgdb-metadata-wasm-plugin`).

This is a real, separate repo on purpose - same reasoning as `steam-source-wasm-plugin`: a
plugin whose source lives inside the host app's own repo doesn't genuinely exercise the
"install arbitrary third-party code" model the WASM plugin system is for.

This port needed a host primitive that didn't exist yet: `http-request` (method/url/headers/
body). IGDB auth runs through Twitch's OAuth (a `client_credentials` token exchange) and the
IGDB query API itself is POST-based with a `Client-ID` header and a raw Apicalypse query-string
body - none of that fits the existing GET-only, header-less `http-get`. Added to the shared
`wit/plugin.wit` host interface and implemented in the host app's `wasm_plugins.rs`, alongside
a third `bindgen!`-generated world (`metadata-plugin-world`) for this plugin kind.

Requires a Twitch-issued client id/secret (create an app at
[dev.twitch.tv](https://dev.twitch.tv/console/apps) to get one, same as the built-in version
required) - set them in Concourse's Settings under this plugin's row (rendered from
`plugin.json`'s `settingsSchema`, a generic form the host builds for any WASM plugin that
declares one; no custom UI code needed on either side).

## Building

```sh
rustup target add wasm32-wasip1   # once
cargo install cargo-component     # once
cargo component build
```

Output: `target/wasm32-wasip1/debug/igdb_metadata_wasm_plugin.wasm`.

## Installing into a running Concourse

Either build locally (above) or grab the prebuilt `.wasm` + `plugin.json` from this repo's
[Releases](https://github.com/smh0505/igdb-metadata-wasm-plugin/releases) - CI (`.github/workflows/publish.yml`) publishes a new release
automatically whenever `plugin.json`'s `version` is bumped on `main`. Concourse's Settings ->
Metadata Provider tab -> Add Plugin also accepts a Release's `plugin.json` URL directly
(metadata-kind plugins install by URL, same as source plugins) - the latest one always lives
at:

```
https://github.com/smh0505/igdb-metadata-wasm-plugin/releases/latest/download/plugin.json
```

Copy the compiled `.wasm` and `plugin.json` into
`<app data dir>/wasm-plugins/metadata/igdb-wasm/` (Windows:
`%APPDATA%\com.bloppy.concourse\wasm-plugins\metadata\igdb-wasm\`). It'll show up in Settings'
Plugins panel under the Metadata Provider tab next time the app starts, as **IGDB**.

## Versioning

Plain SemVer (`Cargo.toml` + `plugin.json`'s `version`), independent of Concourse's own
milestone-tracked version - patch for fixes, minor for backward-compatible new capabilities,
major for breaking manifest/WIT interface changes. Full convention:
[`.claude/CLAUDE.md`](https://github.com/smh0505/Concourse/blob/main/.claude/CLAUDE.md) (Plugin Versioning) in the main [Concourse](https://github.com/smh0505/Concourse) repo.
