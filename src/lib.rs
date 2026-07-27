//! WASM `MetadataProviderPlugin` for IGDB, ported from the game-library-client's built-in
//! `igdb.rs`. Provides description/release date/genres (no cover/background art - that's
//! SteamGridDB's job, see `sgdb-metadata-wasm-plugin`).
//!
//! Requires a Twitch-issued client id/secret (IGDB auth runs through Twitch), set via this
//! plugin's `settingsSchema`-declared `client_id`/`client_secret` settings (see `plugin.json`)
//! - read back here through `host::settings-get`, namespaced by the host per plugin id.

#[allow(warnings)]
mod bindings;

use bindings::exports::gamelib::plugin::metadata_plugin::{Guest, MetadataResult};
use bindings::gamelib::plugin::host;

struct IgdbPlugin;

#[derive(serde::Deserialize)]
struct TwitchTokenResponse {
    access_token: String,
}

#[derive(serde::Deserialize)]
struct IgdbGame {
    summary: Option<String>,
    first_release_date: Option<i64>,
    genres: Option<Vec<IgdbGenre>>,
}

#[derive(serde::Deserialize)]
struct IgdbGenre {
    name: String,
}

/// Civil-from-days algorithm (Howard Hinnant), converts a Unix timestamp to a Y-M-D string.
fn unix_to_date(timestamp: i64) -> String {
    let z = timestamp.div_euclid(86400) + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn credentials() -> Result<(String, String), String> {
    let client_id = host::settings_get("client_id")
        .ok_or_else(|| "IGDB Client ID not set - configure it in Settings.".to_string())?;
    let client_secret = host::settings_get("client_secret")
        .ok_or_else(|| "IGDB Client Secret not set - configure it in Settings.".to_string())?;
    Ok((client_id, client_secret))
}

fn get_access_token(client_id: &str, client_secret: &str) -> Result<String, String> {
    let url = format!(
        "https://id.twitch.tv/oauth2/token?client_id={}&client_secret={}&grant_type=client_credentials",
        urlencoding::encode(client_id),
        urlencoding::encode(client_secret)
    );
    let body = host::http_request("POST", &url, &[], None)?;
    let resp: TwitchTokenResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok(resp.access_token)
}

impl Guest for IgdbPlugin {
    fn fetch_metadata(title: String) -> Result<Option<MetadataResult>, String> {
        let (client_id, client_secret) = credentials()?;
        let token = get_access_token(&client_id, &client_secret)?;

        let query = format!(
            "search \"{}\"; fields summary,first_release_date,genres.name; limit 1;",
            title.replace('"', "")
        );
        let headers = [
            ("Client-ID".to_string(), client_id.clone()),
            ("Authorization".to_string(), format!("Bearer {}", token)),
        ];
        let body = host::http_request(
            "POST",
            "https://api.igdb.com/v4/games",
            &headers,
            Some(&query),
        )?;

        let games: Vec<IgdbGame> = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let Some(game) = games.into_iter().next() else {
            return Ok(None);
        };

        Ok(Some(MetadataResult {
            description: game.summary,
            release_date: game.first_release_date.map(unix_to_date),
            genres: game
                .genres
                .unwrap_or_default()
                .into_iter()
                .map(|g| g.name)
                .collect(),
            cover_art_url: None,
            background_art_url: None,
        }))
    }
}

bindings::export!(IgdbPlugin with_types_in bindings);
