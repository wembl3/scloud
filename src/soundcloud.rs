use anyhow::{anyhow, Result};
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_CLIENT_ID: &str = "Pb72ranhoyt6gw7hM7TkzUItXlMWSNSo";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrackUser {
    pub username: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TranscodingFormat {
    pub protocol: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Transcoding {
    pub url: String,
    pub format: TranscodingFormat,
    #[serde(default)]
    pub snipped: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrackMedia {
    pub transcodings: Vec<Transcoding>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Track {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub artwork_url: Option<String>,
    pub user: TrackUser,
    pub media: TrackMedia,
}

impl Track {
    pub fn get_artwork_url(&self) -> Option<String> {
        let raw = self.artwork_url.as_deref().or(self.user.avatar_url.as_deref())?;
        Some(raw.replace("-large.", "-t500x500."))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Playlist {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub track_count: u64,
    #[serde(default)]
    pub duration: u64,
    pub user: Option<TrackUser>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserProfile {
    pub id: u64,
    pub username: String,
}

#[derive(Debug, Deserialize)]
struct LibraryItem {
    pub playlist: Option<Playlist>,
}

#[derive(Debug, Deserialize)]
struct LibraryResponse {
    pub collection: Vec<LibraryItem>,
}

#[derive(Debug, Deserialize)]
struct LikeItem {
    pub track: Option<Track>,
}

#[derive(Debug, Deserialize)]
struct LikesResponse {
    pub collection: Vec<LikeItem>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    pub collection: Vec<Track>,
}

#[derive(Debug, Deserialize)]
struct StreamResolveResponse {
    pub url: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RawPlaylistResponse {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub tracks: Vec<serde_json::Value>,
}

use crate::theme::ThemeName;

fn default_true() -> bool {
    true
}

fn default_volume() -> f64 {
    85.0
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub oauth_token: Option<String>,
    #[serde(default = "default_true")]
    pub download_covers: bool,
    #[serde(default = "default_true")]
    pub autoplay: bool,
    #[serde(default = "default_true")]
    pub cava_enabled: bool,
    #[serde(default)]
    pub theme: ThemeName,
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(default = "default_true")]
    pub theme_background: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            oauth_token: None,
            download_covers: true,
            autoplay: true,
            cava_enabled: true,
            theme: ThemeName::Btop,
            volume: 85.0,
            theme_background: true,
        }
    }
}

pub struct SoundCloud {
    client: Client,
    pub client_id: String,
    pub oauth_token: Option<String>,
    pub user_profile: Option<UserProfile>,
}

impl SoundCloud {
    pub async fn new() -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0")
            .build()
            .unwrap_or_default();

        let client_id = Self::extract_client_id(&client)
            .await
            .unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());

        let mut oauth_token = Self::load_token_from_config();
        if oauth_token.is_none() && !Self::config_path().exists() {
            if let Some(token) = Self::try_extract_firefox_token() {
                let _ = Self::save_token_to_config(&token);
                oauth_token = Some(token);
            }
        }

        let mut sc = Self {
            client,
            client_id,
            oauth_token,
            user_profile: None,
        };

        if sc.oauth_token.is_some() {
            sc.user_profile = sc.fetch_me().await.ok();
        }

        sc
    }

    pub fn config_path() -> PathBuf {
        dirs_config().join("sc-player").join("config.json")
    }

    pub fn load_config() -> Config {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<Config>(&content) {
                    return config;
                }
            }
        }
        Config::default()
    }

    pub fn save_config(config: &Config) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(config)?)?;
        Ok(())
    }

    pub fn favorites_path() -> PathBuf {
        dirs_config().join("sc-player").join("favorites.json")
    }

    pub fn load_favorites() -> Vec<Track> {
        let path = Self::favorites_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(tracks) = serde_json::from_str::<Vec<Track>>(&content) {
                    return tracks;
                }
            }
        }
        Vec::new()
    }

    pub fn save_favorites(tracks: &[Track]) -> Result<()> {
        let path = Self::favorites_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(tracks)?)?;
        Ok(())
    }

    pub fn load_token_from_config() -> Option<String> {
        Self::load_config().oauth_token
    }

    pub fn save_token_to_config(token: &str) -> Result<()> {
        let mut config = Self::load_config();
        config.oauth_token = Some(token.trim().to_string());
        Self::save_config(&config)
    }

    pub fn clear_config_token() -> Result<()> {
        let mut config = Self::load_config();
        config.oauth_token = None;
        Self::save_config(&config)
    }

    pub fn logout(&mut self) -> Result<()> {
        Self::clear_config_token()?;
        self.oauth_token = None;
        self.user_profile = None;
        Ok(())
    }

    pub async fn login(&mut self, token: &str) -> Result<UserProfile> {
        let trimmed = token.trim();
        let prev_token = self.oauth_token.clone();
        self.oauth_token = Some(trimmed.to_string());
        match self.fetch_me().await {
            Ok(profile) => {
                Self::save_token_to_config(trimmed)?;
                self.user_profile = Some(profile.clone());
                Ok(profile)
            }
            Err(e) => {
                self.oauth_token = prev_token;
                Err(e)
            }
        }
    }

    /// Try to read oauth_token from local Firefox cookies.sqlite
    pub fn try_extract_firefox_token() -> Option<String> {
        let script = r#"
import sqlite3, shutil, os, glob, json

candidates = glob.glob(os.path.expanduser("~/.config/mozilla/firefox/*.default*/cookies.sqlite")) + \
             glob.glob(os.path.expanduser("~/.mozilla/firefox/*.default*/cookies.sqlite"))

for db_src in candidates:
    if not os.path.exists(db_src): continue
    tmp_db = f"/tmp/sc_cookies_{os.getpid()}.sqlite"
    try:
        shutil.copy2(db_src, tmp_db)
        if os.path.exists(db_src + "-wal"): shutil.copy2(db_src + "-wal", tmp_db + "-wal")
        conn = sqlite3.connect(tmp_db)
        row = conn.cursor().execute("SELECT value FROM moz_cookies WHERE host LIKE '%soundcloud%' AND name = 'oauth_token'").fetchone()
        conn.close()
        if row and row[0]:
            print(row[0])
            break
    except:
        pass
    finally:
        for f in [tmp_db, tmp_db + "-wal"]:
            if os.path.exists(f): os.remove(f)
"#;
        let output = std::process::Command::new("python3")
            .arg("-c")
            .arg(script)
            .output()
            .ok()?;

        if output.status.success() {
            let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !token.is_empty() {
                return Some(token);
            }
        }
        None
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(ref token) = self.oauth_token {
            if let Ok(val) = HeaderValue::from_str(&format!("OAuth {}", token)) {
                headers.insert(AUTHORIZATION, val);
            }
        }
        headers
    }

    pub async fn extract_client_id(client: &Client) -> Result<String> {
        let html = client.get("https://soundcloud.com").send().await?.text().await?;
        let script_re = Regex::new(r#"https://a-v2\.sndcdn\.com/assets/[^"]+\.js"#)?;
        let mut script_urls: Vec<String> = script_re
            .find_iter(&html)
            .map(|m| m.as_str().to_string())
            .collect();

        script_urls.reverse();

        let id_re = Regex::new(r#"client_id[:=]"([a-zA-Z0-9]{32})""#)?;
        for url in script_urls.into_iter().take(6) {
            if let Ok(js_resp) = client.get(&url).send().await {
                if let Ok(js) = js_resp.text().await {
                    if let Some(caps) = id_re.captures(&js) {
                        if let Some(id) = caps.get(1) {
                            return Ok(id.as_str().to_string());
                        }
                    }
                }
            }
        }

        Ok(DEFAULT_CLIENT_ID.to_string())
    }

    /// Fetch profile of authenticated user
    pub async fn fetch_me(&self) -> Result<UserProfile> {
        let url = format!("https://api-v2.soundcloud.com/me?client_id={}", self.client_id);
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?
            .json::<UserProfile>()
            .await?;
        Ok(resp)
    }

    /// Fetch user's saved/created playlists
    pub async fn get_user_playlists(&self) -> Result<Vec<Playlist>> {
        let url = format!(
            "https://api-v2.soundcloud.com/me/library/all?client_id={}&limit=50",
            self.client_id
        );

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?
            .json::<LibraryResponse>()
            .await?;

        let playlists: Vec<Playlist> = resp
            .collection
            .into_iter()
            .filter_map(|item| item.playlist)
            .collect();

        Ok(playlists)
    }

    /// Fetch user's liked tracks from SoundCloud API
    pub async fn fetch_user_likes(&self) -> Result<Vec<Track>> {
        let profile = self.user_profile.as_ref().ok_or_else(|| anyhow!("Not logged in"))?;
        let url = format!(
            "https://api-v2.soundcloud.com/users/{}/likes?client_id={}&limit=50",
            profile.id, self.client_id
        );

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?
            .json::<LikesResponse>()
            .await?;

        let tracks: Vec<Track> = resp
            .collection
            .into_iter()
            .filter_map(|item| item.track)
            .collect();

        Ok(tracks)
    }

    /// Fetch full tracks for a playlist (resolving any stub IDs)
    pub async fn get_playlist_tracks(&self, playlist_id: u64) -> Result<Vec<Track>> {
        let url = format!(
            "https://api-v2.soundcloud.com/playlists/{}?client_id={}",
            playlist_id, self.client_id
        );

        let raw: RawPlaylistResponse = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?
            .json()
            .await?;

        let mut tracks: Vec<Track> = Vec::new();
        let mut stub_ids: Vec<u64> = Vec::new();

        for val in raw.tracks {
            if val.get("media").is_some() && val.get("title").is_some() {
                if let Ok(track) = serde_json::from_value::<Track>(val) {
                    tracks.push(track);
                }
            } else if let Some(id) = val.get("id").and_then(|v| v.as_u64()) {
                stub_ids.push(id);
            }
        }

        // Batch load stubs in chunks of 50
        for chunk in stub_ids.chunks(50) {
            let ids_str = chunk
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<String>>()
                .join(",");

            let batch_url = format!(
                "https://api-v2.soundcloud.com/tracks?ids={}&client_id={}",
                ids_str, self.client_id
            );

            if let Ok(resp) = self
                .client
                .get(&batch_url)
                .headers(self.auth_headers())
                .send()
                .await
            {
                if let Ok(mut loaded_tracks) = resp.json::<Vec<Track>>().await {
                    tracks.append(&mut loaded_tracks);
                }
            }
        }

        Ok(tracks)
    }

    /// Search tracks
    pub async fn search_tracks(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        let url = format!(
            "https://api-v2.soundcloud.com/search/tracks?q={}&client_id={}&limit={}",
            urlencoding::encode(query),
            self.client_id,
            limit
        );

        let resp: SearchResponse = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?
            .json()
            .await?;
        Ok(resp.collection)
    }

    /// Spotify-like Autoplay: Get related tracks for track ID
    pub async fn get_related_tracks(&self, track_id: u64, limit: usize) -> Result<Vec<Track>> {
        let url = format!(
            "https://api-v2.soundcloud.com/tracks/{}/related?client_id={}&limit={}",
            track_id, self.client_id, limit
        );

        let resp: SearchResponse = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?
            .json()
            .await?;
        Ok(resp.collection)
    }

    /// Resolve playable stream URL
    pub async fn resolve_stream_url(&self, track: &Track) -> Result<String> {
        let non_snipped: Vec<&Transcoding> = track
            .media
            .transcodings
            .iter()
            .filter(|t| !t.snipped)
            .collect();

        let candidates = if !non_snipped.is_empty() {
            non_snipped
        } else {
            track.media.transcodings.iter().collect()
        };

        let chosen = candidates
            .iter()
            .find(|t| t.format.protocol == "progressive" && t.format.mime_type.contains("mpeg"))
            .or_else(|| candidates.iter().find(|t| t.format.protocol == "hls"))
            .or_else(|| candidates.first())
            .ok_or_else(|| anyhow!("No playable transcoding found for track {}", track.id))?;

        let resolve_url = format!("{}?client_id={}", chosen.url, self.client_id);
        let resp: StreamResolveResponse = self
            .client
            .get(&resolve_url)
            .headers(self.auth_headers())
            .send()
            .await?
            .json()
            .await?;

        Ok(resp.url)
    }
}

fn dirs_config() -> PathBuf {
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(config_home)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config")
    }
}

pub fn dirs_cache() -> PathBuf {
    if let Ok(cache_home) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(cache_home)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".cache")
    }
}

pub fn covers_cache_dir() -> PathBuf {
    dirs_cache().join("sc-player").join("covers")
}

pub fn count_cached_covers() -> usize {
    let cache_dir = covers_cache_dir();
    if let Ok(entries) = fs::read_dir(cache_dir) {
        entries.flatten().count()
    } else {
        0
    }
}

pub async fn download_track_cover(track_id: u64, artwork_url: &str) -> Option<PathBuf> {
    let cache_dir = covers_cache_dir();
    if fs::create_dir_all(&cache_dir).is_err() {
        return None;
    }
    let file_path = cache_dir.join(format!("{}.jpg", track_id));
    if file_path.exists() {
        if let Ok(meta) = fs::metadata(&file_path) {
            if meta.len() > 0 {
                return Some(file_path);
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    let resp = client.get(artwork_url)
        .header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64)")
        .send()
        .await
        .ok()?;

    if resp.status().is_success() {
        let bytes = resp.bytes().await.ok()?;
        if fs::write(&file_path, bytes).is_ok() {
            return Some(file_path);
        }
    }
    None
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut encoded = String::new();
        for b in s.bytes() {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
                encoded.push(b as char);
            } else if b == b' ' {
                encoded.push('+');
            } else {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
        encoded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_artwork_url_transformation() {
        let track = Track {
            id: 12345,
            title: "Test Song".to_string(),
            duration: 180000,
            artwork_url: Some("https://i1.sndcdn.com/artworks-xyz-large.jpg".to_string()),
            user: TrackUser {
                username: "Artist".to_string(),
                avatar_url: Some("https://i1.sndcdn.com/avatars-abc-large.jpg".to_string()),
            },
            media: TrackMedia { transcodings: vec![] },
        };

        assert_eq!(
            track.get_artwork_url(),
            Some("https://i1.sndcdn.com/artworks-xyz-t500x500.jpg".to_string())
        );

        // Fallback to avatar if artwork_url is None
        let track_no_art = Track {
            id: 67890,
            title: "Test Song 2".to_string(),
            duration: 120000,
            artwork_url: None,
            user: TrackUser {
                username: "Artist".to_string(),
                avatar_url: Some("https://i1.sndcdn.com/avatars-abc-large.jpg".to_string()),
            },
            media: TrackMedia { transcodings: vec![] },
        };

        assert_eq!(
            track_no_art.get_artwork_url(),
            Some("https://i1.sndcdn.com/avatars-abc-t500x500.jpg".to_string())
        );
    }

    #[test]
    fn test_config_backward_compatibility() {
        // Old config with only oauth_token
        let old_json = r#"{"oauth_token": "my-secret-token"}"#;
        let config: Config = serde_json::from_str(old_json).unwrap();
        assert_eq!(config.oauth_token, Some("my-secret-token".to_string()));
        assert!(config.download_covers);
        assert!(config.autoplay);
        assert!(config.cava_enabled);
        assert_eq!(config.theme, ThemeName::Btop);
        assert_eq!(config.volume, 85.0);
        assert!(config.theme_background);
    }
}
