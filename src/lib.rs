//! WASM `MetadataProviderPlugin` for IGDB, ported from the game-library-client's built-in
//! `igdb.rs`. Provides description/release date/genres (no cover/background art - that's
//! SteamGridDB's job, see `sgdb-metadata-wasm-plugin`).
//!
//! Requires a Twitch-issued client id/secret (IGDB auth runs through Twitch), set via this
//! plugin's `settingsSchema`-declared `client_id`/`client_secret` settings (see `plugin.json`)
//! - read back here through `host::settings-get`, namespaced by the host per plugin id.
//!
//! Two-step search-candidates/fetch-metadata-by-id split (metadata-plugin interface v2) rather
//! than one direct fetch - lets the host disambiguate when IGDB's own search returns more than
//! one plausible match instead of always committing to the first (relevance-ranked, not
//! necessarily correct) result.
//!
//! Only listings whose `name` is an exact case-insensitive match to the query become
//! candidates at all, same reasoning as `rawg-metadata-wasm-plugin`: IGDB's `search` keyword
//! ranks by its own relevance score, not exactness, so blindly offering every top-N result as
//! a "candidate" surfaced the picker far more than genuinely necessary - a query with no
//! exact-name match returns zero candidates (left blank by the host) rather than a pile of
//! only-loosely-related guesses.

#[allow(warnings)]
mod bindings;

use bindings::exports::gamelib::plugin::metadata_plugin::{Guest, MetadataCandidate, MetadataResult};
use bindings::gamelib::plugin::host;

struct IgdbPlugin;

#[derive(serde::Deserialize)]
struct TwitchTokenResponse {
    access_token: String,
}

#[derive(serde::Deserialize)]
struct IgdbSearchResult {
    id: u64,
    name: String,
    cover: Option<IgdbCover>,
}

#[derive(serde::Deserialize)]
struct IgdbCover {
    image_id: String,
}

/// IGDB's documented thumbnail URL pattern (`t_thumb` size) - the search response only ever
/// gives back `cover.image_id`, the actual URL has to be built from it.
fn igdb_thumbnail_url(image_id: &str) -> String {
    format!("https://images.igdb.com/igdb/image/upload/t_thumb/{}.jpg", image_id)
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

/// Re-authenticates on every call rather than caching a token across search-candidates/
/// fetch-metadata-by-id - each WASM instantiation is short-lived and stateless (a fresh
/// instance per host command), so there's no in-plugin place to cache it anyway.
fn authenticated_headers() -> Result<(String, [(String, String); 2]), String> {
    let (client_id, client_secret) = credentials()?;
    let token = get_access_token(&client_id, &client_secret)?;
    let headers = [
        ("Client-ID".to_string(), client_id.clone()),
        ("Authorization".to_string(), format!("Bearer {}", token)),
    ];
    Ok((client_id, headers))
}

impl Guest for IgdbPlugin {
    fn search_candidates(title: String) -> Result<Vec<MetadataCandidate>, String> {
        let (_client_id, headers) = authenticated_headers()?;

        let query = format!(
            "search \"{}\"; fields name,cover.image_id; limit 10;",
            title.replace('"', "")
        );
        let body = host::http_request("POST", "https://api.igdb.com/v4/games", &headers, Some(&query))?;
        let games: Vec<IgdbSearchResult> = serde_json::from_str(&body).map_err(|e| e.to_string())?;

        Ok(games
            .into_iter()
            .filter(|g| g.name.eq_ignore_ascii_case(&title))
            .map(|g| MetadataCandidate {
                id: g.id.to_string(),
                label: g.name,
                image_url: g.cover.map(|c| igdb_thumbnail_url(&c.image_id)),
            })
            .collect())
    }

    fn fetch_metadata_by_id(id: String) -> Result<Option<MetadataResult>, String> {
        let numeric_id: u64 = id.parse().map_err(|_| format!("Invalid IGDB id: {}", id))?;
        let (_client_id, headers) = authenticated_headers()?;

        let query = format!(
            "fields summary,first_release_date,genres.name; where id = {};",
            numeric_id
        );
        let body = host::http_request("POST", "https://api.igdb.com/v4/games", &headers, Some(&query))?;
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
