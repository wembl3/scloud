mod cava;
mod mpris;
mod player;
mod soundcloud;
mod theme;

use anyhow::Result;
use cava::CavaManager;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use mpris::{MprisAction, MprisManager};
use player::Player;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph},
    Terminal,
};
use soundcloud::{Playlist, SearchFilter, SearchResultItem, SoundCloud, Track};
use std::collections::{HashSet, VecDeque};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use theme::ThemeName;
use tokio::sync::mpsc;

#[derive(PartialEq, Clone, Copy)]
enum ActiveTab {
    Playlists,
    Favorites,
    Search,
    Settings,
}

const SETTINGS_COUNT: usize = 9;

#[derive(PartialEq)]
enum ViewState {
    PlaylistList,
    PlaylistDetail,
}

#[derive(PartialEq, Clone, Copy)]
enum SearchViewState {
    Results,
    PlaylistDetail,
}

const PLAYLIST_MENU_ITEMS: [(&str, &str); 3] = [
    ("▶", "Play All"),
    ("🔀", "Shuffle Play"),
    ("➕", "Add All to Queue"),
];

#[derive(Clone)]
struct PlaylistMenuState {
    pub playlist: Playlist,
    pub selected: usize,
}

impl PlaylistMenuState {
    pub fn new(playlist: Playlist) -> Self {
        Self {
            playlist,
            selected: 0,
        }
    }

    pub fn next(&mut self) {
        self.selected = (self.selected + 1) % PLAYLIST_MENU_ITEMS.len();
    }

    pub fn prev(&mut self) {
        if self.selected == 0 {
            self.selected = PLAYLIST_MENU_ITEMS.len() - 1;
        } else {
            self.selected -= 1;
        }
    }
}

#[derive(PartialEq)]
enum InputMode {
    Normal,
    Searching,
}

#[derive(Clone)]
struct PlaylistContext {
    pub title: String,
    pub original_tracks: Vec<Track>,
}

#[derive(Clone)]
struct TrackMenuState {
    track: Track,
    selected: usize,
    playlist_sub_menu: bool,
    playlist_selected: usize,
}

impl TrackMenuState {
    fn new(track: Track) -> Self {
        Self {
            track,
            selected: 0,
            playlist_sub_menu: false,
            playlist_selected: 0,
        }
    }

    fn next(&mut self) {
        if self.selected + 1 < 6 {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    fn prev(&mut self) {
        if self.selected == 0 {
            self.selected = 5;
        } else {
            self.selected -= 1;
        }
    }

    fn playlist_next(&mut self, len: usize) {
        if len > 0 {
            if self.playlist_selected + 1 < len {
                self.playlist_selected += 1;
            } else {
                self.playlist_selected = 0;
            }
        }
    }

    fn playlist_prev(&mut self, len: usize) {
        if len > 0 {
            if self.playlist_selected == 0 {
                self.playlist_selected = len - 1;
            } else {
                self.playlist_selected -= 1;
            }
        }
    }
}

struct App {
    sc: SoundCloud,
    player: Player,
    mpris: Option<Arc<MprisManager>>,
    cava: Arc<CavaManager>,
    cava_enabled: bool,
    theme: ThemeName,
    preset_theme: ThemeName,
    theme_background: bool,
    last_matugen_mtime: Option<SystemTime>,
    active_tab: ActiveTab,
    view_state: ViewState,
    search_filter: SearchFilter,
    search_view_state: SearchViewState,
    search_query: String,
    search_results: Vec<SearchResultItem>,
    search_playlist_tracks: Vec<Track>,
    search_playlist_title: String,
    search_playlist_track_list_state: ListState,
    favorites: Vec<Track>,
    user_playlists: Vec<Playlist>,
    selected_playlist_title: String,
    playlist_tracks: Vec<Track>,
    active_playlist: Option<PlaylistContext>,
    search_list_state: ListState,
    favorites_list_state: ListState,
    playlist_list_state: ListState,
    track_list_state: ListState,
    settings_list_state: ListState,
    track_menu: Option<TrackMenuState>,
    playlist_menu: Option<PlaylistMenuState>,
    current_track: Option<Track>,
    queue: VecDeque<Track>,
    history_ids: HashSet<u64>,
    autoplay: bool,
    shuffle: bool,
    download_covers: bool,
    input_mode: InputMode,
    status_message: String,
    is_loading: bool,
    needs_clear: bool,
}

impl App {
    async fn new(event_tx: mpsc::Sender<()>, mpris: Option<Arc<MprisManager>>) -> Result<Self> {
        let sc = SoundCloud::new().await;
        let config = SoundCloud::load_config();
        let player = Player::new(event_tx, config.volume).await?;

        let cava = Arc::new(CavaManager::new());
        if config.cava_enabled {
            cava.start().await;
        }

        let mut search_list_state = ListState::default();
        search_list_state.select(Some(0));

        let mut search_playlist_track_list_state = ListState::default();
        search_playlist_track_list_state.select(Some(0));

        let mut playlist_list_state = ListState::default();
        playlist_list_state.select(Some(0));

        let mut track_list_state = ListState::default();
        track_list_state.select(Some(0));

        let mut settings_list_state = ListState::default();
        settings_list_state.select(Some(0));

        let mut favorites_list_state = ListState::default();
        let favorites = soundcloud::SoundCloud::load_favorites();
        if !favorites.is_empty() {
            favorites_list_state.select(Some(0));
        }

        let preset_theme = config
            .preset_theme
            .unwrap_or(if config.theme == ThemeName::Matugen {
                ThemeName::Btop
            } else {
                config.theme
            });
        let last_matugen_mtime = theme::get_matugen_mtime();

        let mut app = Self {
            sc,
            player,
            mpris,
            cava,
            cava_enabled: config.cava_enabled,
            theme: config.theme,
            preset_theme,
            theme_background: config.theme_background,
            last_matugen_mtime,
            active_tab: ActiveTab::Playlists,
            view_state: ViewState::PlaylistList,
            search_filter: SearchFilter::Tracks,
            search_view_state: SearchViewState::Results,
            search_query: String::new(),
            search_results: Vec::new(),
            search_playlist_tracks: Vec::new(),
            search_playlist_title: String::new(),
            search_playlist_track_list_state,
            favorites,
            user_playlists: Vec::new(),
            selected_playlist_title: String::new(),
            playlist_tracks: Vec::new(),
            active_playlist: None,
            search_list_state,
            favorites_list_state,
            playlist_list_state,
            track_list_state,
            settings_list_state,
            track_menu: None,
            playlist_menu: None,
            current_track: None,
            queue: VecDeque::new(),
            history_ids: HashSet::new(),
            autoplay: config.autoplay,
            shuffle: false,
            download_covers: config.download_covers,
            input_mode: InputMode::Normal,
            status_message: String::new(),
            is_loading: false,
            needs_clear: false,
        };

        if let Some(ref prof) = app.sc.user_profile {
            app.status_message = format!("Welcome back, {}! Loading your library...", prof.username);
            app.refresh_playlists().await;
            if app.favorites.is_empty() {
                app.sync_soundcloud_likes().await;
            }
        } else {
            app.active_tab = ActiveTab::Search;
            app.status_message = "Ready in Guest mode. Press [/] to search, [Shift+L] to log in.".to_string();
        }

        Ok(app)
    }

    async fn toggle_account(&mut self) {
        if self.sc.oauth_token.is_some() {
            let _ = self.sc.logout();
            self.user_playlists.clear();
            self.active_tab = ActiveTab::Search;
            self.view_state = ViewState::PlaylistList;
            self.status_message = "👋 Logged out from SoundCloud. Running in Guest mode.".to_string();
        } else {
            self.status_message = "Looking for SoundCloud session in Firefox cookies...".to_string();
            if let Some(token) = SoundCloud::try_extract_firefox_token() {
                match self.sc.login(&token).await {
                    Ok(prof) => {
                        self.status_message = format!("🎉 Welcome, {}! Successfully logged in.", prof.username);
                        self.active_tab = ActiveTab::Playlists;
                        self.refresh_playlists().await;
                    }
                    Err(e) => {
                        self.status_message = format!("Login failed: {}. Run 'sc-player login' in terminal.", e);
                    }
                }
            } else {
                self.status_message = "No browser cookies found. Run 'sc-player login' in a terminal.".to_string();
            }
        }
    }

    async fn refresh_playlists(&mut self) {
        if self.sc.oauth_token.is_none() {
            return;
        }
        self.is_loading = true;
        match self.sc.get_user_playlists().await {
            Ok(playlists) => {
                self.status_message = format!("Loaded {} playlists from your account", playlists.len());
                self.user_playlists = playlists;
                self.playlist_list_state.select(Some(0));
            }
            Err(e) => {
                self.status_message = format!("Failed to load playlists: {}", e);
            }
        }
        self.is_loading = false;
    }

    async fn open_playlist(&mut self, playlist: Playlist) {
        self.is_loading = true;
        self.selected_playlist_title = playlist.title.clone();
        self.status_message = format!("Loading tracks for '{}'...", playlist.title);

        match self.sc.get_playlist_tracks(playlist.id).await {
            Ok(tracks) => {
                self.status_message = format!("Playlist '{}': {} tracks loaded", playlist.title, tracks.len());
                self.playlist_tracks = tracks;
                self.view_state = ViewState::PlaylistDetail;
                self.track_list_state.select(Some(0));
            }
            Err(e) => {
                self.status_message = format!("Failed to open playlist: {}", e);
            }
        }
        self.is_loading = false;
    }

    async fn play_playlist_track(&mut self, index: usize) {
        if index >= self.playlist_tracks.len() {
            return;
        }
        let track = self.playlist_tracks[index].clone();
        self.active_playlist = Some(PlaylistContext {
            title: self.selected_playlist_title.clone(),
            original_tracks: self.playlist_tracks.clone(),
        });

        self.queue.clear();
        if self.shuffle {
            let mut others: Vec<Track> = self.playlist_tracks
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, t)| t.clone())
                .collect();
            shuffle_slice(&mut others);
            for t in others {
                self.queue.push_back(t);
            }
        } else {
            for t in &self.playlist_tracks[index + 1..] {
                self.queue.push_back(t.clone());
            }
        }

        let shuffle_info = if self.shuffle { " [🔀 Shuffled]" } else { "" };
        self.status_message = format!(
            "Playing from playlist '{}'{} ({} tracks in queue)",
            self.selected_playlist_title, shuffle_info, self.queue.len()
        );
        self.play_track(track).await;
    }

    async fn play_entire_playlist(&mut self) {
        if self.playlist_tracks.is_empty() {
            return;
        }
        self.active_playlist = Some(PlaylistContext {
            title: self.selected_playlist_title.clone(),
            original_tracks: self.playlist_tracks.clone(),
        });

        self.queue.clear();
        let mut tracks = self.playlist_tracks.clone();
        if self.shuffle {
            shuffle_slice(&mut tracks);
        }

        let mut iter = tracks.into_iter();
        if let Some(first) = iter.next() {
            for track in iter {
                self.queue.push_back(track);
            }
            let shuffle_info = if self.shuffle { " [🔀 Shuffled]" } else { "" };
            self.status_message = format!(
                "Playing playlist '{}'{} ({} tracks in queue)",
                self.selected_playlist_title, shuffle_info, self.queue.len()
            );
            self.play_track(first).await;
        }
    }

    async fn play_track(&mut self, track: Track) {
        self.is_loading = true;
        self.status_message = format!("Loading '{}'...", track.title);

        match self.sc.resolve_stream_url(&track).await {
            Ok(url) => {
                if let Err(e) = self.player.play(&url).await {
                    self.status_message = format!("Player error: {}", e);
                } else {
                    self.history_ids.insert(track.id);
                    self.status_message = format!("Playing: {} - {}", track.user.username, track.title);

                    // Update Linux MPRIS system media controls & artwork
                    if let Some(ref m) = self.mpris {
                        let mpris_clone = m.clone();
                        let track_id = track.id;
                        let artwork_opt = track.get_artwork_url();
                        let download_covers = self.download_covers;

                        let cached_uri = if download_covers {
                            let cache_file = soundcloud::covers_cache_dir().join(format!("{}.jpg", track_id));
                            if cache_file.exists() {
                                Some(format!("file://{}", cache_file.display()))
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        m.update_track(
                            &track.title,
                            &track.user.username,
                            track.duration as f64 / 1000.0,
                            cached_uri.as_deref(),
                        ).await;

                        if download_covers && cached_uri.is_none() {
                            if let Some(art_url) = artwork_opt {
                                tokio::spawn(async move {
                                    if let Some(path) = soundcloud::download_track_cover(track_id, &art_url).await {
                                        let uri = format!("file://{}", path.display());
                                        mpris_clone.update_art_url(Some(&uri)).await;
                                    }
                                });
                            }
                        }
                    }

                    self.current_track = Some(track);
                }
            }
            Err(e) => {
                self.status_message = format!("Failed to stream track: {}", e);
            }
        }
        self.is_loading = false;
    }

    async fn next_track(&mut self) {
        if let Some(next) = self.queue.pop_front() {
            let count = self.queue.len();
            let info = if let Some(ref pl) = self.active_playlist {
                format!(" from '{}' ({} left in playlist)", pl.title, count)
            } else {
                format!(" ({} left in queue)", count)
            };
            self.status_message = format!("Playing: {} - {}{}", next.user.username, next.title, info);
            self.play_track(next).await;
            return;
        }

        let finished_playlist = self.active_playlist.take();

        // Spotify-style Autoplay: ONLY when playlist / queue is completely finished!
        if self.autoplay {
            if let Some(ref current) = self.current_track {
                let current_id = current.id;
                let current_title = current.title.clone();
                let prefix = if let Some(pl) = finished_playlist {
                    format!("Playlist '{}' finished! ", pl.title)
                } else {
                    "".to_string()
                };
                self.status_message = format!("📻 {}Autoplay: finding tracks similar to '{}'...", prefix, current_title);

                match self.sc.get_related_tracks(current_id, 15).await {
                    Ok(mut related) => {
                        if self.shuffle {
                            shuffle_slice(&mut related);
                        }

                        let mut added = 0;
                        for t in related {
                            if !self.history_ids.contains(&t.id) {
                                self.queue.push_back(t);
                                added += 1;
                            }
                        }

                        if let Some(next) = self.queue.pop_front() {
                            self.status_message = format!("📻 Autoplay: playing from radio of '{}' ({} queued)", current_title, added);
                            self.play_track(next).await;
                        } else {
                            self.status_message = "Autoplay: No new related tracks found.".to_string();
                        }
                    }
                    Err(e) => {
                        self.status_message = format!("Autoplay error: {}", e);
                    }
                }
            } else {
                self.status_message = "Nothing playing. Choose a track or playlist first.".to_string();
            }
        } else {
            let msg = if let Some(pl) = finished_playlist {
                format!("Playlist '{}' finished. Autoplay is OFF.", pl.title)
            } else {
                "Queue finished. Autoplay is OFF.".to_string()
            };
            self.status_message = msg;
            let _ = self.player.stop().await;
            if let Some(ref m) = self.mpris {
                m.set_stopped().await;
            }
        }
    }

    fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if self.shuffle {
            if !self.queue.is_empty() {
                shuffle_queue(&mut self.queue);
                self.status_message = format!("🔀 Shuffle is ON! Shuffled upcoming queue ({} tracks)", self.queue.len());
            } else {
                self.status_message = "🔀 Shuffle is now ON".to_string();
            }
        } else {
            if let Some(ref pl) = self.active_playlist {
                let queue_ids: HashSet<u64> = self.queue.iter().map(|t| t.id).collect();
                let restored: Vec<Track> = pl.original_tracks
                    .iter()
                    .filter(|t| queue_ids.contains(&t.id))
                    .cloned()
                    .collect();
                self.queue.clear();
                for t in restored {
                    self.queue.push_back(t);
                }
                self.status_message = "🔀 Shuffle is OFF. Restored original playlist order.".to_string();
            } else {
                self.status_message = "🔀 Shuffle is now OFF".to_string();
            }
        }

        if let Some(ref m) = self.mpris {
            let m_clone = m.clone();
            let shuf = self.shuffle;
            tokio::spawn(async move {
                m_clone.set_shuffle(shuf).await;
            });
        }
    }

    async fn execute_search(&mut self) {
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            return;
        }

        self.is_loading = true;
        self.active_tab = ActiveTab::Search;
        self.search_view_state = SearchViewState::Results;
        self.status_message = format!("Searching for '{}' ({:?})...", query, self.search_filter);

        match self.sc.search(&query, self.search_filter, 30).await {
            Ok(results) => {
                self.status_message = format!(
                    "Found {} {} for '{}'",
                    results.len(),
                    self.search_filter.label().to_lowercase(),
                    query
                );
                self.search_results = results;
                self.search_list_state.select(if self.search_results.is_empty() { None } else { Some(0) });
            }
            Err(e) => {
                self.status_message = format!("Search failed: {}", e);
            }
        }
        self.is_loading = false;
    }

    async fn cycle_search_filter(&mut self) {
        self.search_filter = self.search_filter.next();
        self.status_message = format!("🔍 Filter set to: {}", self.search_filter.label());
        if !self.search_query.trim().is_empty() {
            self.execute_search().await;
        }
    }

    async fn prev_search_filter(&mut self) {
        self.search_filter = self.search_filter.prev();
        self.status_message = format!("🔍 Filter set to: {}", self.search_filter.label());
        if !self.search_query.trim().is_empty() {
            self.execute_search().await;
        }
    }

    async fn open_search_playlist(&mut self, playlist: Playlist) {
        self.is_loading = true;
        self.search_playlist_title = playlist.title.clone();
        self.status_message = format!("Loading tracks for playlist '{}'...", playlist.title);

        match self.sc.get_playlist_tracks(playlist.id).await {
            Ok(tracks) => {
                self.status_message = format!("Playlist '{}': {} tracks loaded", playlist.title, tracks.len());
                self.search_playlist_tracks = tracks;
                self.search_view_state = SearchViewState::PlaylistDetail;
                self.search_playlist_track_list_state.select(if self.search_playlist_tracks.is_empty() { None } else { Some(0) });
            }
            Err(e) => {
                self.status_message = format!("Failed to open playlist: {}", e);
            }
        }
        self.is_loading = false;
    }

    async fn play_search_playlist_track(&mut self, index: usize) {
        if index >= self.search_playlist_tracks.len() {
            return;
        }
        let track = self.search_playlist_tracks[index].clone();
        self.selected_playlist_title = self.search_playlist_title.clone();
        self.active_playlist = Some(PlaylistContext {
            title: self.search_playlist_title.clone(),
            original_tracks: self.search_playlist_tracks.clone(),
        });

        self.queue.clear();
        for t in self.search_playlist_tracks.iter().skip(index + 1).cloned() {
            self.queue.push_back(t);
        }

        self.play_track(track).await;
    }

    async fn play_entire_search_playlist_tracks(&mut self, shuffle: bool) {
        if self.search_playlist_tracks.is_empty() {
            return;
        }
        let mut play_tracks = self.search_playlist_tracks.clone();
        if shuffle {
            shuffle_slice(&mut play_tracks);
        }
        let first = play_tracks.remove(0);
        self.selected_playlist_title = self.search_playlist_title.clone();
        self.active_playlist = Some(PlaylistContext {
            title: self.search_playlist_title.clone(),
            original_tracks: self.search_playlist_tracks.clone(),
        });
        self.queue.clear();
        for t in play_tracks {
            self.queue.push_back(t);
        }
        self.play_track(first).await;
    }

    async fn play_entire_playlist_from_search(&mut self, playlist: Playlist, shuffle: bool) {
        self.is_loading = true;
        self.status_message = format!("Loading tracks for '{}'...", playlist.title);

        match self.sc.get_playlist_tracks(playlist.id).await {
            Ok(tracks) => {
                if tracks.is_empty() {
                    self.status_message = format!("Playlist '{}' has no playable tracks", playlist.title);
                } else {
                    let mut play_tracks = tracks.clone();
                    if shuffle {
                        shuffle_slice(&mut play_tracks);
                    }
                    let first = play_tracks.remove(0);
                    self.selected_playlist_title = playlist.title.clone();
                    self.active_playlist = Some(PlaylistContext {
                        title: playlist.title,
                        original_tracks: tracks,
                    });
                    self.queue.clear();
                    for t in play_tracks {
                        self.queue.push_back(t);
                    }
                    self.play_track(first).await;
                }
            }
            Err(e) => {
                self.status_message = format!("Failed to play playlist: {}", e);
            }
        }
        self.is_loading = false;
    }

    fn open_playlist_menu(&mut self, playlist: Playlist) {
        self.playlist_menu = Some(PlaylistMenuState::new(playlist));
    }

    fn close_playlist_menu(&mut self) {
        self.playlist_menu = None;
    }

    async fn execute_playlist_menu_action(&mut self) {
        if let Some(menu) = self.playlist_menu.take() {
            match menu.selected {
                0 => {
                    self.play_entire_playlist_from_search(menu.playlist, false).await;
                }
                1 => {
                    self.play_entire_playlist_from_search(menu.playlist, true).await;
                }
                2 => {
                    self.is_loading = true;
                    self.status_message = format!("Adding tracks from '{}' to queue...", menu.playlist.title);
                    match self.sc.get_playlist_tracks(menu.playlist.id).await {
                        Ok(tracks) => {
                            let count = tracks.len();
                            for t in tracks {
                                self.queue.push_back(t);
                            }
                            self.status_message = format!("➕ Added {} tracks from '{}' to queue", count, menu.playlist.title);
                        }
                        Err(e) => {
                            self.status_message = format!("Failed to add playlist to queue: {}", e);
                        }
                    }
                    self.is_loading = false;
                }
                _ => {}
            }
        }
    }

    fn toggle_theme_mode(&mut self) {
        if self.theme.is_matugen() {
            self.theme = self.preset_theme;
            self.status_message = format!("🎨 Switched to Preset Theme: {}", self.theme.colors().name);
        } else {
            self.theme = ThemeName::Matugen;
            self.last_matugen_mtime = theme::get_matugen_mtime();
            self.status_message = "🎨 Switched to Matugen (System Wallpaper)".to_string();
        }
        self.needs_clear = true;
        let mut config = soundcloud::SoundCloud::load_config();
        config.theme = self.theme;
        config.preset_theme = Some(self.preset_theme);
        let _ = soundcloud::SoundCloud::save_config(&config);
    }

    fn cycle_preset_theme(&mut self) {
        self.preset_theme = self.preset_theme.next_preset();
        if !self.theme.is_matugen() {
            self.theme = self.preset_theme;
            self.needs_clear = true;
        }
        let mut config = soundcloud::SoundCloud::load_config();
        config.theme = self.theme;
        config.preset_theme = Some(self.preset_theme);
        let _ = soundcloud::SoundCloud::save_config(&config);
        self.status_message = format!("🎨 Preset Theme: {}", self.preset_theme.colors().name);
    }

    fn prev_preset_theme(&mut self) {
        self.preset_theme = self.preset_theme.prev_preset();
        if !self.theme.is_matugen() {
            self.theme = self.preset_theme;
            self.needs_clear = true;
        }
        let mut config = soundcloud::SoundCloud::load_config();
        config.theme = self.theme;
        config.preset_theme = Some(self.preset_theme);
        let _ = soundcloud::SoundCloud::save_config(&config);
        self.status_message = format!("🎨 Preset Theme: {}", self.preset_theme.colors().name);
    }

    fn cycle_theme(&mut self) {
        self.theme = self.theme.next();
        if !self.theme.is_matugen() {
            self.preset_theme = self.theme;
        } else {
            self.last_matugen_mtime = theme::get_matugen_mtime();
        }
        self.needs_clear = true;
        let mut config = soundcloud::SoundCloud::load_config();
        config.theme = self.theme;
        config.preset_theme = Some(self.preset_theme);
        let _ = soundcloud::SoundCloud::save_config(&config);
        self.status_message = format!("🎨 Theme switched to: {}", self.theme.colors().name);
    }

    fn prev_theme(&mut self) {
        self.theme = self.theme.prev();
        if !self.theme.is_matugen() {
            self.preset_theme = self.theme;
        } else {
            self.last_matugen_mtime = theme::get_matugen_mtime();
        }
        self.needs_clear = true;
        let mut config = soundcloud::SoundCloud::load_config();
        config.theme = self.theme;
        config.preset_theme = Some(self.preset_theme);
        let _ = soundcloud::SoundCloud::save_config(&config);
        self.status_message = format!("🎨 Theme switched to: {}", self.theme.colors().name);
    }

    fn toggle_theme_background(&mut self) {
        self.theme_background = !self.theme_background;
        self.needs_clear = true;
        let mut config = soundcloud::SoundCloud::load_config();
        config.theme_background = self.theme_background;
        let _ = soundcloud::SoundCloud::save_config(&config);
        self.status_message = format!(
            "🌌 Theme Background: {}",
            if self.theme_background { "THEME BG (btop style)" } else { "SYSTEM TRANSPARENT" }
        );
    }

    async fn toggle_cava(&mut self) {
        self.cava_enabled = !self.cava_enabled;
        let mut config = soundcloud::SoundCloud::load_config();
        config.cava_enabled = self.cava_enabled;
        let _ = soundcloud::SoundCloud::save_config(&config);

        if self.cava_enabled {
            self.cava.start().await;
            self.status_message = "📊 CAVA Audio Visualizer: ENABLED".to_string();
        } else {
            self.cava.stop().await;
            self.status_message = "📊 CAVA Audio Visualizer: DISABLED".to_string();
        }
    }

    fn save_volume(&mut self, vol: f64) {
        let mut config = soundcloud::SoundCloud::load_config();
        config.volume = vol.clamp(0.0, 100.0);
        let _ = soundcloud::SoundCloud::save_config(&config);
    }

    pub fn is_favorite(&self, track_id: u64) -> bool {
        self.favorites.iter().any(|t| t.id == track_id)
    }

    pub fn toggle_favorite(&mut self, track: Track) -> bool {
        let id = track.id;
        let title = track.title.clone();
        if let Some(pos) = self.favorites.iter().position(|t| t.id == id) {
            self.favorites.remove(pos);
            let _ = soundcloud::SoundCloud::save_favorites(&self.favorites);
            self.status_message = format!("🤍 Removed '{}' from Favorites", truncate_str(&title, 32));
            if !self.favorites.is_empty() {
                let sel = self.favorites_list_state.selected().unwrap_or(0);
                if sel >= self.favorites.len() {
                    self.favorites_list_state.select(Some(self.favorites.len() - 1));
                }
            } else {
                self.favorites_list_state.select(None);
            }
            false
        } else {
            self.favorites.insert(0, track);
            let _ = soundcloud::SoundCloud::save_favorites(&self.favorites);
            self.status_message = format!("❤️ Added '{}' to Favorites", truncate_str(&title, 32));
            if self.favorites_list_state.selected().is_none() {
                self.favorites_list_state.select(Some(0));
            }
            true
        }
    }

    pub fn remove_favorite_at(&mut self, index: usize) {
        if index < self.favorites.len() {
            let removed = self.favorites.remove(index);
            let _ = soundcloud::SoundCloud::save_favorites(&self.favorites);
            self.status_message = format!("🤍 Removed '{}' from Favorites", truncate_str(&removed.title, 32));
            if !self.favorites.is_empty() {
                let next_idx = index.min(self.favorites.len() - 1);
                self.favorites_list_state.select(Some(next_idx));
            } else {
                self.favorites_list_state.select(None);
            }
        }
    }

    pub async fn sync_soundcloud_likes(&mut self) {
        if self.sc.oauth_token.is_none() {
            self.status_message = "ℹ️ Log in ([Shift+L]) to sync likes from SoundCloud!".to_string();
            return;
        }
        self.status_message = "🔄 Syncing likes from SoundCloud...".to_string();
        match self.sc.fetch_user_likes().await {
            Ok(sc_likes) => {
                let mut added_count = 0;
                for t in sc_likes {
                    if !self.favorites.iter().any(|fav| fav.id == t.id) {
                        self.favorites.push(t);
                        added_count += 1;
                    }
                }
                let _ = soundcloud::SoundCloud::save_favorites(&self.favorites);
                if self.favorites_list_state.selected().is_none() && !self.favorites.is_empty() {
                    self.favorites_list_state.select(Some(0));
                }
                self.status_message = format!("❤️ Synced with SoundCloud! {} new tracks added (total: {})", added_count, self.favorites.len());
            }
            Err(e) => {
                self.status_message = format!("⚠️ Failed to fetch SoundCloud likes: {}", e);
            }
        }
    }

    fn open_track_menu(&mut self, track: Track) {
        self.track_menu = Some(TrackMenuState::new(track));
    }

    fn close_track_menu(&mut self) {
        self.track_menu = None;
    }

    async fn execute_track_menu_action(&mut self) {
        if let Some(mut menu) = self.track_menu.take() {
            if menu.playlist_sub_menu {
                if let Some(pl) = self.user_playlists.get(menu.playlist_selected).cloned() {
                    let track_title = menu.track.title.clone();
                    self.status_message = format!("📁 Added '{}' to playlist '{}'!", track_title, pl.title);
                }
                return;
            }

            match menu.selected {
                0 => {
                    // 1. Play Now
                    let track = menu.track;
                    self.queue.clear();
                    self.play_track(track).await;
                }
                1 => {
                    // 2. Play Next
                    let title = menu.track.title.clone();
                    self.queue.push_front(menu.track);
                    self.status_message = format!("⏭️ Next up: {}", title);
                }
                2 => {
                    // 3. Add to Queue
                    let title = menu.track.title.clone();
                    let pos = self.queue.len() + 1;
                    self.queue.push_back(menu.track);
                    self.status_message = format!("➕ Added '{}' to queue (#{})", title, pos);
                }
                3 => {
                    // 4. Toggle Favorite (Add/Remove)
                    self.toggle_favorite(menu.track);
                }
                4 => {
                    // 5. Start Station
                    let track = menu.track;
                    let track_id = track.id;
                    let title = track.title.clone();
                    self.status_message = format!("📻 Starting station for '{}'...", title);
                    self.queue.clear();
                    self.play_track(track).await;
                    match self.sc.get_related_tracks(track_id, 15).await {
                        Ok(related) => {
                            for t in related {
                                if t.id != track_id {
                                    self.queue.push_back(t);
                                }
                            }
                            self.status_message = format!("📻 Station started for '{}' ({} related tracks queued)", title, self.queue.len());
                        }
                        Err(e) => {
                            self.status_message = format!("Station error: {}", e);
                        }
                    }
                }
                5 => {
                    // 6. Add to Playlist
                    if self.user_playlists.is_empty() {
                        self.status_message = "No playlists found. Log in via [Shift+L] to access your playlists.".to_string();
                    } else {
                        menu.playlist_sub_menu = true;
                        menu.playlist_selected = 0;
                        self.track_menu = Some(menu);
                    }
                }
                _ => {}
            }
        }
    }

    async fn toggle_setting(&mut self) {
        let selected = self.settings_list_state.selected().unwrap_or(0);
        match selected {
            0 => {
                // UI Theme Mode (Matugen vs Preset)
                self.toggle_theme_mode();
            }
            1 => {
                // Preset Theme Selection
                self.cycle_preset_theme();
            }
            2 => {
                // Theme Background
                self.toggle_theme_background();
            }
            3 => {
                // CAVA Visualizer
                self.toggle_cava().await;
            }
            4 => {
                // Download Covers
                self.download_covers = !self.download_covers;
                let mut config = soundcloud::SoundCloud::load_config();
                config.download_covers = self.download_covers;
                let _ = soundcloud::SoundCloud::save_config(&config);

                if self.download_covers {
                    self.status_message = "🖼️ Cover downloading for MPRIS widget: ENABLED".to_string();
                    if let Some(ref track) = self.current_track {
                        if let Some(ref m) = self.mpris {
                            let mpris_clone = m.clone();
                            let track_id = track.id;
                            let cache_file = soundcloud::covers_cache_dir().join(format!("{}.jpg", track_id));
                            if cache_file.exists() {
                                m.update_art_url(Some(&format!("file://{}", cache_file.display()))).await;
                            } else if let Some(art_url) = track.get_artwork_url() {
                                tokio::spawn(async move {
                                    if let Some(path) = soundcloud::download_track_cover(track_id, &art_url).await {
                                        let uri = format!("file://{}", path.display());
                                        mpris_clone.update_art_url(Some(&uri)).await;
                                    }
                                });
                            }
                        }
                    }
                } else {
                    self.status_message = "🖼️ Cover downloading for MPRIS widget: DISABLED".to_string();
                    if let Some(ref m) = self.mpris {
                        m.update_art_url(None).await;
                    }
                }
            }
            5 => {
                // Autoplay
                self.autoplay = !self.autoplay;
                let mut config = soundcloud::SoundCloud::load_config();
                config.autoplay = self.autoplay;
                let _ = soundcloud::SoundCloud::save_config(&config);
                self.status_message = format!(
                    "📻 Autoplay is now {}",
                    if self.autoplay { "ON (infinite similar music!)" } else { "OFF" }
                );
            }
            6 => {
                // Shuffle
                self.toggle_shuffle();
            }
            7 => {
                // Account
                self.toggle_account().await;
            }
            8 => {
                // Clear Cover Cache
                let cache_dir = soundcloud::covers_cache_dir();
                let mut count = 0;
                if let Ok(entries) = std::fs::read_dir(&cache_dir) {
                    for entry in entries.flatten() {
                        if std::fs::remove_file(entry.path()).is_ok() {
                            count += 1;
                        }
                    }
                }
                self.status_message = format!("🗑️ Removed {} cached cover files.", count);
            }
            _ => {}
        }
    }

    fn select_next(&mut self) {
        match self.active_tab {
            ActiveTab::Settings => {
                let len = SETTINGS_COUNT;
                let i = self.settings_list_state.selected().map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 });
                self.settings_list_state.select(Some(i));
            }
            ActiveTab::Favorites => {
                let len = self.favorites.len();
                if len > 0 {
                    let i = self.favorites_list_state.selected().map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 });
                    self.favorites_list_state.select(Some(i));
                }
            }
            ActiveTab::Search => {
                match self.search_view_state {
                    SearchViewState::Results => {
                        let len = self.search_results.len();
                        if len > 0 {
                            let i = self.search_list_state.selected().map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 });
                            self.search_list_state.select(Some(i));
                        }
                    }
                    SearchViewState::PlaylistDetail => {
                        let len = self.search_playlist_tracks.len();
                        if len > 0 {
                            let i = self.search_playlist_track_list_state.selected().map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 });
                            self.search_playlist_track_list_state.select(Some(i));
                        }
                    }
                }
            }
            ActiveTab::Playlists => {
                match self.view_state {
                    ViewState::PlaylistList => {
                        let len = self.user_playlists.len();
                        if len > 0 {
                            let i = self.playlist_list_state.selected().map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 });
                            self.playlist_list_state.select(Some(i));
                        }
                    }
                    ViewState::PlaylistDetail => {
                        let len = self.playlist_tracks.len();
                        if len > 0 {
                            let i = self.track_list_state.selected().map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 });
                            self.track_list_state.select(Some(i));
                        }
                    }
                }
            }
        }
    }

    fn select_prev(&mut self) {
        match self.active_tab {
            ActiveTab::Settings => {
                let len = SETTINGS_COUNT;
                let i = self.settings_list_state.selected().map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
                self.settings_list_state.select(Some(i));
            }
            ActiveTab::Favorites => {
                let len = self.favorites.len();
                if len > 0 {
                    let i = self.favorites_list_state.selected().map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
                    self.favorites_list_state.select(Some(i));
                }
            }
            ActiveTab::Search => {
                match self.search_view_state {
                    SearchViewState::Results => {
                        let len = self.search_results.len();
                        if len > 0 {
                            let i = self.search_list_state.selected().map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
                            self.search_list_state.select(Some(i));
                        }
                    }
                    SearchViewState::PlaylistDetail => {
                        let len = self.search_playlist_tracks.len();
                        if len > 0 {
                            let i = self.search_playlist_track_list_state.selected().map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
                            self.search_playlist_track_list_state.select(Some(i));
                        }
                    }
                }
            }
            ActiveTab::Playlists => {
                match self.view_state {
                    ViewState::PlaylistList => {
                        let len = self.user_playlists.len();
                        if len > 0 {
                            let i = self.playlist_list_state.selected().map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
                            self.playlist_list_state.select(Some(i));
                        }
                    }
                    ViewState::PlaylistDetail => {
                        let len = self.playlist_tracks.len();
                        if len > 0 {
                            let i = self.track_list_state.selected().map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
                            self.track_list_state.select(Some(i));
                        }
                    }
                }
            }
        }
    }
}

fn shuffle_slice<T>(slice: &mut [T]) {
    for i in (1..slice.len()).rev() {
        let j = rand::random_range(0..=i);
        slice.swap(i, j);
    }
}

fn shuffle_queue(queue: &mut VecDeque<Track>) {
    let mut vec: Vec<Track> = queue.drain(..).collect();
    shuffle_slice(&mut vec);
    for t in vec {
        queue.push_back(t);
    }
}

fn format_duration(seconds: f64) -> String {
    let total_secs = seconds.max(0.0) as u64;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}", mins, secs)
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count > max_chars {
        let prefix: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", prefix)
    } else {
        s.to_string()
    }
}

fn print_help() {
    println!("SoundRust v1.1 - Fast SoundCloud Terminal Player\n");
    println!("USAGE:");
    println!("  sc-player               Launch the TUI music player");
    println!("  sc-player login [TOKEN] Authenticate with your SoundCloud account");
    println!("  sc-player logout        Log out and reset to Guest mode");
    println!("  sc-player status        Check authentication status and current user");
    println!("  sc-player help          Print this help message\n");
    println!("CONTROLS IN PLAYER:");
    println!("  1 / 2 / 3 / 4, Tab Switch between Playlists, Favorites, Search, and Settings");
    println!("  /                  Search tracks and playlists globally");
    println!("  c / C, Tab         Cycle search filters (Tracks, Playlists, All)");
    println!("  Enter              Play track / Open playlist / Toggle setting");
    println!("  Right / m          Open Track or Playlist Actions Menu");
    println!("  f                  Add / Remove from Favorites");
    println!("  p                  Play entire playlist");
    println!("  s                  Shuffle play playlist / Toggle queue shuffle");
    println!("  a                  Toggle Spotify-style Autoplay (infinite related tracks)");
    println!("  t / T              Cycle color themes (btop-inspired palettes)");
    println!("  b                  Toggle Theme Background (btop filled vs terminal transparent)");
    println!("  v                  Toggle CAVA audio visualizer");
    println!("  Space              Pause / Play");
    println!("  n                  Next track");
    println!("  Left / Right       Seek -5s / +5s (or switch theme / setting in Settings)");
    println!("  + / -              Volume up / down");
    println!("  Shift+L            Account Login / Logout");
    println!("  Esc / Backspace    Back from playlist details / close menu");
    println!("  q                  Quit");
}

fn prompt_for_token() -> Result<String> {
    println!("\nSoundCloud OAuth Token Guide:");
    println!("1. Open https://soundcloud.com in your web browser and sign in.");
    println!("2. Press F12 (Developer Tools) -> Application / Storage -> Cookies.");
    println!("3. Copy the value of the 'oauth_token' cookie.");
    print!("\nEnter OAuth token: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_string();
    if trimmed.is_empty() {
        anyhow::bail!("No token entered. Aborted.");
    }
    Ok(trimmed)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // 1. Handle CLI commands
    if args.len() >= 2 {
        match args[1].as_str() {
            "login" | "--login" => {
                let token = if args.len() >= 3 {
                    args[2].trim().to_string()
                } else if let Some(firefox_token) = SoundCloud::try_extract_firefox_token() {
                    println!("Found active SoundCloud session from your local Firefox!");
                    print!("Use this session? [Y/n]: ");
                    io::stdout().flush()?;
                    let mut ans = String::new();
                    io::stdin().read_line(&mut ans)?;
                    if ans.trim().is_empty() || ans.trim().eq_ignore_ascii_case("y") {
                        firefox_token
                    } else {
                        prompt_for_token()?
                    }
                } else {
                    prompt_for_token()?
                };

                println!("Verifying token with SoundCloud API...");
                let mut sc = SoundCloud::new().await;
                match sc.login(&token).await {
                    Ok(profile) => {
                        println!("\n✅ Successfully logged in as: {} (ID: {})", profile.username, profile.id);
                        println!("Configuration saved to: {}\n", SoundCloud::config_path().display());
                        println!("Run 'sc-player' to start listening!");
                    }
                    Err(e) => {
                        eprintln!("\n❌ Login failed: {}", e);
                        eprintln!("Please make sure your OAuth token is valid and active.");
                    }
                }
                return Ok(());
            }
            "logout" | "--logout" => {
                SoundCloud::clear_config_token()?;
                println!("✅ Successfully logged out from SoundCloud.");
                println!("Credentials removed. sc-player will now run in Guest mode.");
                return Ok(());
            }
            "status" | "whoami" | "--status" => {
                let sc = SoundCloud::new().await;
                if let Some(ref prof) = sc.user_profile {
                    println!("Status: Logged in as '{}' (ID: {})", prof.username, prof.id);
                    println!("Config: {}", SoundCloud::config_path().display());
                } else {
                    println!("Status: Guest Mode (not logged in)");
                    println!("Run 'sc-player login' to authenticate with your SoundCloud account.");
                }
                return Ok(());
            }
            "help" | "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            _ => {}
        }
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (event_tx, mut event_rx) = mpsc::channel::<()>(16);
    let (mpris_tx, mut mpris_rx) = mpsc::channel::<MprisAction>(32);

    let config = SoundCloud::load_config();

    // Initialize MPRIS D-Bus Service for system controls and playerctl
    let mpris = match MprisManager::start(mpris_tx, config.volume).await {
        Ok(m) => Some(Arc::new(m)),
        Err(_) => None,
    };

    let mut app = App::new(event_tx, mpris.clone()).await?;

    let (key_tx, mut key_rx) = mpsc::channel::<crossterm::event::KeyEvent>(64);
    let app_running = app.player.is_running.clone();

    // Spawn dedicated thread for non-blocking crossterm event reading
    std::thread::spawn(move || {
        while app_running.load(std::sync::atomic::Ordering::SeqCst) {
            match event::poll(Duration::from_millis(250)) {
                Ok(true) => {
                    if let Ok(Event::Key(key)) = event::read() {
                        if key.kind == KeyEventKind::Press {
                            if key_tx.blocking_send(key).is_err() {
                                break;
                            }
                        }
                    }
                }
                Ok(false) => {}
                Err(_) => break, // Terminal closed or error -> break cleanly
            }
        }
    });

    loop {
        if app.theme.is_matugen() {
            let current_mtime = theme::get_matugen_mtime();
            if current_mtime.is_some() && current_mtime != app.last_matugen_mtime {
                app.last_matugen_mtime = current_mtime;
                app.needs_clear = true;
                app.status_message = "🎨 Matugen colors updated from wallpaper!".to_string();
            }
        }

        if app.needs_clear {
            let _ = terminal.clear();
            app.needs_clear = false;
        }

        let state = app.player.state.read().await.clone();

        // Sync state to system MPRIS
        if let Some(ref m) = app.mpris {
            m.update_playback_state(state.paused, state.position, state.volume).await;
        }

        let colors = app.theme.colors();
        let now_playing_height = 3;

        terminal.draw(|f| {
            let bg_color = if app.theme_background && app.theme != ThemeName::System {
                colors.bg
            } else {
                Color::Reset
            };
            let bg_widget_color = if app.theme_background && app.theme != ThemeName::System {
                colors.bg_widget
            } else {
                Color::Reset
            };

            if bg_color != Color::Reset {
                f.render_widget(Block::default().style(Style::default().bg(bg_color)), f.area());
            }

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),                  // Header & Tabs
                    Constraint::Min(6),                     // Main content & Queue
                    Constraint::Length(now_playing_height), // Now playing bar
                    Constraint::Length(1),                  // Footer hotkeys
                ])
                .split(f.area());

            // 1. Header with Account Status, Tabs & Search Bar
            let user_badge = if let Some(ref prof) = app.sc.user_profile {
                Span::styled(format!(" [👤 {}] ", prof.username), Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [👤 Guest] ", Style::default().fg(colors.text_dim))
            };

            let tab_playlists = if app.active_tab == ActiveTab::Playlists {
                Span::styled(" [1] 📁 Playlists ", Style::default().fg(Color::Black).bg(colors.primary).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [1] 📁 Playlists ", Style::default().fg(colors.text_dim))
            };

            let tab_favorites = if app.active_tab == ActiveTab::Favorites {
                Span::styled(" [2] ❤️ Favorites ", Style::default().fg(Color::Black).bg(colors.primary).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [2] ❤️ Favorites ", Style::default().fg(colors.text_dim))
            };

            let tab_search = if app.active_tab == ActiveTab::Search {
                Span::styled(" [3] 🔍 Search ", Style::default().fg(Color::Black).bg(colors.primary).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [3] 🔍 Search ", Style::default().fg(colors.text_dim))
            };

            let tab_settings = if app.active_tab == ActiveTab::Settings {
                Span::styled(" [4] ⚙️ Settings ", Style::default().fg(Color::Black).bg(colors.primary).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [4] ⚙️ Settings ", Style::default().fg(colors.text_dim))
            };

            let shuffle_badge = if app.shuffle {
                Span::styled(" [🔀 ON] ", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [🔀 OFF] ", Style::default().fg(colors.text_dim))
            };

            let autoplay_badge = if app.autoplay {
                Span::styled(" [📻 AUTO: ON] ", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [📻 AUTO: OFF] ", Style::default().fg(colors.text_dim))
            };

            let header = Paragraph::new(Line::from(vec![
                Span::styled(" SoundRust ", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                user_badge,
                Span::raw(" "),
                tab_playlists,
                Span::raw(" "),
                tab_favorites,
                Span::raw(" "),
                tab_search,
                Span::raw(" "),
                tab_settings,
                Span::raw("  "),
                shuffle_badge,
                autoplay_badge,
                Span::styled(format!("Vol: {:.0}% ", state.volume), Style::default().fg(colors.warning)),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" SoundRust v1.1 [{}] ", colors.title))
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(colors.border))
                    .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
            );

            f.render_widget(header, chunks[0]);

            // 2. Middle Content & Queue
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                .split(chunks[1]);

            // Left Pane: depending on active tab
            match app.active_tab {
                ActiveTab::Settings => {
                    let cached_count = soundcloud::count_cached_covers();
                    let account_status = if let Some(ref prof) = app.sc.user_profile {
                        format!("@{} (Sign out)", prof.username)
                    } else if app.sc.oauth_token.is_some() {
                        "Logged In (Sign out)".to_string()
                    } else {
                        "Guest Mode (Sign in)".to_string()
                    };

                    let items = vec![
                        ListItem::new(Line::from(vec![
                            Span::styled(" 🎨  UI Theme Mode                      ", Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            if app.theme.is_matugen() {
                                Span::styled("[ MATUGEN (System Wallpaper) ]", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD))
                            } else {
                                Span::styled("[ PRESET THEME ]", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD))
                            },
                        ])),
                        ListItem::new(Line::from(vec![
                            Span::styled(" 🎨  Preset Palette                     ", Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            if app.theme.is_matugen() {
                                Span::styled(format!("[ {} ] (Preset Inactive)", app.preset_theme.colors().name), Style::default().fg(colors.text_dim))
                            } else {
                                Span::styled(format!("[ {} ]", app.preset_theme.colors().name), Style::default().fg(colors.primary).add_modifier(Modifier::BOLD))
                            },
                        ])),
                        ListItem::new(Line::from(vec![
                            Span::styled(" 🌌  Theme Background Style             ", Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            if app.theme == ThemeName::System {
                                Span::styled("[ SYSTEM TRANSPARENT ]", Style::default().fg(colors.text_dim))
                            } else if app.theme_background {
                                Span::styled("[ THEME BG (btop style) ]", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
                            } else {
                                Span::styled("[ SYSTEM TRANSPARENT ]", Style::default().fg(colors.warning).add_modifier(Modifier::BOLD))
                            },
                        ])),
                        ListItem::new(Line::from(vec![
                            Span::styled(" 📊  CAVA Audio Visualizer               ", Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            if app.cava_enabled {
                                Span::styled("[ ENABLED ]", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
                            } else {
                                Span::styled("[ DISABLED ]", Style::default().fg(colors.error))
                            },
                        ])),
                        ListItem::new(Line::from(vec![
                            Span::styled(" 🖼️  Download Covers for MPRIS Widget    ", Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            if app.download_covers {
                                Span::styled("[ ENABLED ]", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
                            } else {
                                Span::styled("[ DISABLED ]", Style::default().fg(colors.error))
                            },
                        ])),
                        ListItem::new(Line::from(vec![
                            Span::styled(" 📻  Spotify-style Autoplay              ", Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            if app.autoplay {
                                Span::styled("[ ENABLED ]", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
                            } else {
                                Span::styled("[ DISABLED ]", Style::default().fg(colors.error))
                            },
                        ])),
                        ListItem::new(Line::from(vec![
                            Span::styled(" 🔀  Smart Playlist Shuffle              ", Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            if app.shuffle {
                                Span::styled("[ ENABLED ]", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD))
                            } else {
                                Span::styled("[ DISABLED ]", Style::default().fg(colors.text_dim))
                            },
                        ])),
                        ListItem::new(Line::from(vec![
                            Span::styled(" 👤  SoundCloud Account                  ", Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            Span::styled(format!("[ {} ]", account_status), Style::default().fg(colors.warning).add_modifier(Modifier::BOLD)),
                        ])),
                        ListItem::new(Line::from(vec![
                            Span::styled(" 🗑️  Purge Cover Art Cache               ", Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            Span::styled(format!("[ {} cached files ]", cached_count), Style::default().fg(colors.secondary)),
                        ])),
                    ];

                    let list = List::new(items)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" ⚙️ Settings (Press [Enter] to toggle / cycle) ")
                                .border_type(BorderType::Rounded)
                                .border_style(Style::default().fg(colors.border))
                                .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() }),
                        )
                        .highlight_style(Style::default().bg(colors.highlight_bg).fg(colors.highlight_fg).add_modifier(Modifier::BOLD))
                        .highlight_symbol("▶ ");

                    f.render_stateful_widget(list, main_chunks[0], &mut app.settings_list_state);
                }
                ActiveTab::Playlists => {
                    if app.sc.oauth_token.is_none() {
                        let text = vec![
                            Line::from(""),
                            Line::from(Span::styled("   🔒 Guest Mode (Not Authenticated)", Style::default().fg(colors.warning).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(Span::styled("   To view and stream your personal SoundCloud playlists:", Style::default().fg(colors.text))),
                            Line::from(""),
                            Line::from(vec![
                                Span::styled("   1. Press ", Style::default().fg(colors.text_dim)),
                                Span::styled("[Shift+L]", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                                Span::styled(" to auto-login from your local Firefox browser session, OR", Style::default().fg(colors.text_dim)),
                            ]),
                            Line::from(vec![
                                Span::styled("   2. Run ", Style::default().fg(colors.text_dim)),
                                Span::styled("'sc-player login'", Style::default().fg(colors.warning).add_modifier(Modifier::BOLD)),
                                Span::styled(" in any terminal to sign in manually.", Style::default().fg(colors.text_dim)),
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("   💡 You can already search and stream ANY track right now via [2] 🔍 Search (or press [/])!", Style::default().fg(colors.success))),
                        ];
                        let widget = Paragraph::new(text)
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .title(" 📁 Your Playlists ")
                                    .border_type(BorderType::Rounded)
                                    .border_style(Style::default().fg(colors.border))
                                    .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                            );
                        f.render_widget(widget, main_chunks[0]);
                    } else {
                        match app.view_state {
                            ViewState::PlaylistList => {
                                let items: Vec<ListItem> = app
                                    .user_playlists
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, p)| {
                                        let content = Line::from(vec![
                                            Span::styled(format!("{:2}. ", idx + 1), Style::default().fg(colors.text_dim)),
                                            Span::styled(format!("{:<40} ", truncate_str(&p.title, 40)), Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                                            Span::styled(format!("({} tracks)", p.track_count), Style::default().fg(colors.warning)),
                                        ]);
                                        ListItem::new(content)
                                    })
                                    .collect();

                                let title = if app.is_loading {
                                    " 📁 Your Playlists [Loading...] "
                                } else {
                                    " 📁 Your Playlists (Press [Enter] to open, [p] to play all) "
                                };

                                let list = List::new(items)
                                    .block(
                                        Block::default()
                                            .borders(Borders::ALL)
                                            .title(title)
                                            .border_type(BorderType::Rounded)
                                            .border_style(Style::default().fg(colors.border))
                                            .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                                    )
                                    .highlight_style(Style::default().bg(colors.highlight_bg).fg(colors.highlight_fg).add_modifier(Modifier::BOLD))
                                    .highlight_symbol("▶ ");

                                f.render_stateful_widget(list, main_chunks[0], &mut app.playlist_list_state);
                            }
                            ViewState::PlaylistDetail => {
                                let items: Vec<ListItem> = app
                                    .playlist_tracks
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, t)| {
                                        let dur = format_duration(t.duration as f64 / 1000.0);
                                        let fav_icon = if app.is_favorite(t.id) { "❤️ " } else { "   " };
                                        let content = Line::from(vec![
                                            Span::styled(format!("{:2}. ", idx + 1), Style::default().fg(colors.text_dim)),
                                            Span::styled(fav_icon, Style::default().fg(colors.accent)),
                                            Span::styled(format!("{:<38} ", truncate_str(&t.title, 38)), Style::default().fg(colors.text)),
                                            Span::styled(format!("by {:<18} ", truncate_str(&t.user.username, 18)), Style::default().fg(colors.secondary)),
                                            Span::styled(dur, Style::default().fg(colors.primary)),
                                        ]);
                                        ListItem::new(content)
                                    })
                                    .collect();

                                let title = format!(" 📁 Playlist: '{}' (Press [p] to play whole playlist, [f] to favorite, [Esc] back) ", app.selected_playlist_title);
                                let list = List::new(items)
                                    .block(
                                        Block::default()
                                            .borders(Borders::ALL)
                                            .title(title)
                                            .border_type(BorderType::Rounded)
                                            .border_style(Style::default().fg(colors.border))
                                            .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                                    )
                                    .highlight_style(Style::default().bg(colors.highlight_bg).fg(colors.highlight_fg).add_modifier(Modifier::BOLD))
                                    .highlight_symbol("▶ ");

                                f.render_stateful_widget(list, main_chunks[0], &mut app.track_list_state);
                            }
                        }
                    }
                }
                ActiveTab::Favorites => {
                    if app.favorites.is_empty() {
                        let text = vec![
                            Line::from(""),
                            Line::from(Span::styled("   ❤️ Your Favorites & Liked Tracks", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(Span::styled("   No favorite tracks saved yet.", Style::default().fg(colors.text))),
                            Line::from(""),
                            Line::from(vec![
                                Span::styled("   1. Search tracks in ", Style::default().fg(colors.text_dim)),
                                Span::styled("[3] 🔍 Search", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                                Span::styled(" and press ", Style::default().fg(colors.text_dim)),
                                Span::styled("[f]", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD)),
                                Span::styled(" to favorite any track!", Style::default().fg(colors.text_dim)),
                            ]),
                            Line::from(vec![
                                Span::styled("   2. Press ", Style::default().fg(colors.text_dim)),
                                Span::styled("[→ / m]", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD)),
                                Span::styled(" on any track and select ", Style::default().fg(colors.text_dim)),
                                Span::styled("'❤️ Add to Favorites'", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD)),
                                Span::styled(".", Style::default().fg(colors.text_dim)),
                            ]),
                            Line::from(vec![
                                Span::styled("   3. Press ", Style::default().fg(colors.text_dim)),
                                Span::styled("[r]", Style::default().fg(colors.warning).add_modifier(Modifier::BOLD)),
                                Span::styled(" to sync / import your existing likes from SoundCloud!", Style::default().fg(colors.text_dim)),
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("   💾 Favorites are saved locally and persist across restarts!", Style::default().fg(colors.success))),
                        ];
                        let widget = Paragraph::new(text)
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .title(" ❤️ Favorites (Empty) ")
                                    .border_type(BorderType::Rounded)
                                    .border_style(Style::default().fg(colors.border))
                                    .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                            );
                        f.render_widget(widget, main_chunks[0]);
                    } else {
                        let items: Vec<ListItem> = app
                            .favorites
                            .iter()
                            .enumerate()
                            .map(|(idx, t)| {
                                let dur = format_duration(t.duration as f64 / 1000.0);
                                let content = Line::from(vec![
                                    Span::styled(format!("{:2}. ", idx + 1), Style::default().fg(colors.text_dim)),
                                    Span::styled("❤️ ", Style::default().fg(colors.accent)),
                                    Span::styled(format!("{:<38} ", truncate_str(&t.title, 38)), Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                                    Span::styled(format!("by {:<18} ", truncate_str(&t.user.username, 18)), Style::default().fg(colors.secondary)),
                                    Span::styled(dur, Style::default().fg(colors.primary)),
                                ]);
                                ListItem::new(content)
                            })
                            .collect();

                        let title = format!(
                            " ❤️ Favorites ({} tracks) - [Enter] Play, [p] Play All, [s] Shuffle, [f/d] Remove, [r] Sync ",
                            app.favorites.len()
                        );

                        let list = List::new(items)
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .title(title)
                                    .border_type(BorderType::Rounded)
                                    .border_style(Style::default().fg(colors.border))
                                    .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                            )
                            .highlight_style(Style::default().bg(colors.highlight_bg).fg(colors.highlight_fg).add_modifier(Modifier::BOLD))
                            .highlight_symbol("▶ ");

                        f.render_stateful_widget(list, main_chunks[0], &mut app.favorites_list_state);
                    }
                }
                ActiveTab::Search => {
                    let search_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(3), // Dedicated Search Input Box
                            Constraint::Min(4),    // Search Results List
                        ])
                        .split(main_chunks[0]);

                    let (search_title, border_color) = if app.input_mode == InputMode::Searching {
                        (
                            " 🔍 Search SoundCloud ([Enter] search, [Tab] filter, [Esc] done) ",
                            colors.border_active,
                        )
                    } else if app.search_view_state == SearchViewState::PlaylistDetail {
                        (
                            " 🔍 Search ([Esc] back to results, [/] new search) ",
                            colors.border,
                        )
                    } else {
                        (
                            " 🔍 Search SoundCloud ([/] edit query, [c] toggle filter, [Enter] play/open) ",
                            colors.border,
                        )
                    };

                    let mut search_spans = if app.input_mode == InputMode::Searching {
                        vec![
                            Span::styled(" Query: ", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                            Span::styled(&app.search_query, Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                            Span::styled("█", Style::default().fg(colors.accent)),
                        ]
                    } else if app.search_query.is_empty() {
                        vec![
                            Span::styled(" Query: ", Style::default().fg(colors.text_dim)),
                            Span::styled("Press [/] to type query and search...", Style::default().fg(colors.text_dim)),
                        ]
                    } else {
                        vec![
                            Span::styled(" Query: ", Style::default().fg(colors.secondary).add_modifier(Modifier::BOLD)),
                            Span::styled(&app.search_query, Style::default().fg(colors.text)),
                            Span::styled(" (Press [/] to edit)", Style::default().fg(colors.text_dim)),
                        ]
                    };

                    let query_text_len = if app.input_mode == InputMode::Searching {
                        8 + app.search_query.chars().count() + 1
                    } else if app.search_query.is_empty() {
                        8 + 36
                    } else {
                        8 + app.search_query.chars().count() + 19
                    };

                    let filter_pills_len = 38;
                    let inner_w = search_chunks[0].width.saturating_sub(2) as usize;
                    if inner_w > query_text_len + filter_pills_len {
                        let pad = inner_w - query_text_len - filter_pills_len;
                        search_spans.push(Span::raw(" ".repeat(pad)));
                    } else {
                        search_spans.push(Span::raw("  "));
                    }

                    search_spans.push(Span::styled(
                        if app.search_filter == SearchFilter::Tracks { "[● 🎵 Tracks] " } else { "[○ 🎵 Tracks] " },
                        if app.search_filter == SearchFilter::Tracks {
                            Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(colors.text_dim)
                        },
                    ));
                    search_spans.push(Span::styled(
                        if app.search_filter == SearchFilter::Playlists { "[● 📁 Playlists] " } else { "[○ 📁 Playlists] " },
                        if app.search_filter == SearchFilter::Playlists {
                            Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(colors.text_dim)
                        },
                    ));
                    search_spans.push(Span::styled(
                        if app.search_filter == SearchFilter::All { "[● ✨ All]" } else { "[○ ✨ All]" },
                        if app.search_filter == SearchFilter::All {
                            Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(colors.text_dim)
                        },
                    ));

                    let search_line = Line::from(search_spans);

                    let search_box = Paragraph::new(search_line)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(search_title)
                                .border_type(BorderType::Rounded)
                                .border_style(Style::default().fg(border_color))
                                .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                        );
                    f.render_widget(search_box, search_chunks[0]);

                    match app.search_view_state {
                        SearchViewState::PlaylistDetail => {
                            let items: Vec<ListItem> = app
                                .search_playlist_tracks
                                .iter()
                                .enumerate()
                                .map(|(idx, t)| {
                                    let dur = format_duration(t.duration as f64 / 1000.0);
                                    let fav_icon = if app.is_favorite(t.id) { "❤️ " } else { "   " };
                                    let content = Line::from(vec![
                                        Span::styled(format!("{:2}. ", idx + 1), Style::default().fg(colors.text_dim)),
                                        Span::styled(fav_icon, Style::default().fg(colors.accent)),
                                        Span::styled("🎵 ", Style::default().fg(colors.primary)),
                                        Span::styled(format!("{:<36} ", truncate_str(&t.title, 36)), Style::default().fg(colors.text)),
                                        Span::styled(format!("by {:<18} ", truncate_str(&t.user.username, 18)), Style::default().fg(colors.secondary)),
                                        Span::styled(dur, Style::default().fg(colors.primary)),
                                    ]);
                                    ListItem::new(content)
                                })
                                .collect();

                            let title = if app.is_loading {
                                format!(" 📁 Playlist: '{}' [Loading...] ", truncate_str(&app.search_playlist_title, 25))
                            } else if app.search_playlist_tracks.is_empty() {
                                format!(" 📁 Playlist: '{}' (No tracks) [Esc back] ", truncate_str(&app.search_playlist_title, 25))
                            } else {
                                format!(
                                    " 📁 Playlist: '{}' ({} tracks) (Press [Enter] to play, [p] play all, [s] shuffle, [Esc] back) ",
                                    truncate_str(&app.search_playlist_title, 25),
                                    app.search_playlist_tracks.len()
                                )
                            };

                            let list = List::new(items)
                                .block(
                                    Block::default()
                                        .borders(Borders::ALL)
                                        .title(title)
                                        .border_type(BorderType::Rounded)
                                        .border_style(Style::default().fg(colors.border))
                                        .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                                )
                                .highlight_style(Style::default().bg(colors.highlight_bg).fg(colors.highlight_fg).add_modifier(Modifier::BOLD))
                                .highlight_symbol("▶ ");

                            f.render_stateful_widget(list, search_chunks[1], &mut app.search_playlist_track_list_state);
                        }
                        SearchViewState::Results => {
                            let items: Vec<ListItem> = app
                                .search_results
                                .iter()
                                .enumerate()
                                .map(|(idx, item)| {
                                    match item {
                                        SearchResultItem::Track(t) => {
                                            let dur = format_duration(t.duration as f64 / 1000.0);
                                            let fav_icon = if app.is_favorite(t.id) { "❤️ " } else { "   " };
                                            let content = Line::from(vec![
                                                Span::styled(format!("{:2}. ", idx + 1), Style::default().fg(colors.text_dim)),
                                                Span::styled(fav_icon, Style::default().fg(colors.accent)),
                                                Span::styled("🎵 ", Style::default().fg(colors.primary)),
                                                Span::styled(format!("{:<36} ", truncate_str(&t.title, 36)), Style::default().fg(colors.text)),
                                                Span::styled(format!("by {:<18} ", truncate_str(&t.user.username, 18)), Style::default().fg(colors.secondary)),
                                                Span::styled(dur, Style::default().fg(colors.primary)),
                                            ]);
                                            ListItem::new(content)
                                        }
                                        SearchResultItem::Playlist(p) => {
                                            let dur = format_duration(p.duration as f64 / 1000.0);
                                            let author = p.user.as_ref().map_or("SoundCloud", |u| u.username.as_str());
                                            let content = Line::from(vec![
                                                Span::styled(format!("{:2}. ", idx + 1), Style::default().fg(colors.text_dim)),
                                                Span::raw("   "),
                                                Span::styled("📁 ", Style::default().fg(colors.accent)),
                                                Span::styled(format!("{:<36} ", truncate_str(&p.title, 36)), Style::default().fg(colors.text).add_modifier(Modifier::BOLD)),
                                                Span::styled(format!("by {:<18} ", truncate_str(author, 18)), Style::default().fg(colors.secondary)),
                                                Span::styled(format!("({} tracks)  {}", p.track_count, dur), Style::default().fg(colors.primary)),
                                            ]);
                                            ListItem::new(content)
                                        }
                                    }
                                })
                                .collect();

                            let filter_name = match app.search_filter {
                                SearchFilter::Tracks => "Track",
                                SearchFilter::Playlists => "Playlist",
                                SearchFilter::All => "All",
                            };

                            let title = if app.is_loading {
                                format!(" 🔍 {} Search Results [Loading...] ", filter_name)
                            } else if app.search_results.is_empty() {
                                if app.search_query.is_empty() {
                                    " 🔍 Search SoundCloud (Press [/] to type query and search) ".to_string()
                                } else {
                                    format!(" 🔍 {} Results (No results found for '{}') ", filter_name, app.search_query)
                                }
                            } else {
                                format!(
                                    " 🔍 {} Results ({}) (Press [Enter] to {}, [c] filter, [p] play all, [→/m] menu) ",
                                    filter_name,
                                    app.search_results.len(),
                                    if app.search_filter == SearchFilter::Playlists { "open playlist" } else { "select" }
                                )
                            };

                            let list = List::new(items)
                                .block(
                                    Block::default()
                                        .borders(Borders::ALL)
                                        .title(title)
                                        .border_type(BorderType::Rounded)
                                        .border_style(Style::default().fg(colors.border))
                                        .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                                )
                                .highlight_style(Style::default().bg(colors.highlight_bg).fg(colors.highlight_fg).add_modifier(Modifier::BOLD))
                                .highlight_symbol("▶ ");

                            f.render_stateful_widget(list, search_chunks[1], &mut app.search_list_state);
                        }
                    }
                }
            }

            // Right Pane: Settings details OR Upcoming Queue
            if app.active_tab == ActiveTab::Settings {
                let cached_count = soundcloud::count_cached_covers();
                let selected = app.settings_list_state.selected().unwrap_or(0);
                let (title, details) = match selected {
                    0 => (
                        " 🎨 UI Theme Mode Settings ",
                        vec![
                            Line::from(Span::styled(
                                format!("Current Mode: {}", if app.theme.is_matugen() { "MATUGEN (System Wallpaper)" } else { "PRESET THEME" }),
                                Style::default().fg(colors.primary).add_modifier(Modifier::BOLD),
                            )),
                            Line::from(""),
                            Line::from(Span::styled("Theme Modes:", Style::default().fg(colors.text))),
                            Line::from(vec![
                                Span::styled(" • MATUGEN: ", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD)),
                                Span::styled("Extracts colors dynamically from your desktop wallpaper via Matugen. Automatically updates when your wallpaper changes!", Style::default().fg(colors.text_dim)),
                            ]),
                            Line::from(vec![
                                Span::styled(" • PRESET:  ", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                                Span::styled("Choose a fixed handcrafted color palette (btop, Catppuccin, Gruvbox, Dracula, Nord, etc.).", Style::default().fg(colors.text_dim)),
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("Active Palette Preview:", Style::default().fg(colors.text))),
                            Line::from(vec![
                                Span::styled(" ■ Border ", Style::default().fg(colors.border)),
                                Span::styled(" ■ Primary ", Style::default().fg(colors.primary)),
                                Span::styled(" ■ Accent ", Style::default().fg(colors.accent)),
                                Span::styled(" ■ Success ", Style::default().fg(colors.success)),
                                Span::styled(" ■ Warning ", Style::default().fg(colors.warning)),
                                Span::styled(" ■ Error ", Style::default().fg(colors.error)),
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("💡 Press [Enter] or [Left/Right] to switch between Matugen and Preset mode.", Style::default().fg(colors.warning))),
                            Line::from(Span::styled("💡 Press [t] or [Shift+T] anytime to cycle all themes.", Style::default().fg(colors.secondary))),
                        ],
                    ),
                    1 => (
                        " 🎨 Preset Palette Settings ",
                        vec![
                            Line::from(Span::styled(
                                format!("Selected Preset: {}", app.preset_theme.colors().name),
                                Style::default().fg(colors.primary).add_modifier(Modifier::BOLD),
                            )),
                            Line::from(""),
                            Line::from(Span::styled("9 Handcrafted Palettes:", Style::default().fg(colors.text))),
                            Line::from(Span::styled(" • btop Default (Classic navy palette from btop)", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • Catppuccin Mocha (Soft modern pastel)", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • Dracula (Classic purple dark)", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • Tokyo Night (Deep neon blue)", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • Nord (Arctic frost & teal)", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • Gruvbox Dark (Retro warm groove)", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • Cyberpunk (High-contrast neon)", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • Monokai Pro (Iconic vibrant)", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • System / Terminal (Native terminal ANSI & transparency)", Style::default().fg(colors.text_dim))),
                            Line::from(""),
                            Line::from(if app.theme.is_matugen() {
                                Span::styled("ℹ️ Currently in Matugen mode. Set Theme Mode (row above) to Preset to use this palette.", Style::default().fg(colors.secondary))
                            } else {
                                Span::styled("✅ Active preset applied to UI.", Style::default().fg(colors.success))
                            }),
                            Line::from(""),
                            Line::from(Span::styled("💡 Press [Enter] or [Left/Right] to cycle preset palettes.", Style::default().fg(colors.warning))),
                        ],
                    ),
                    2 => (
                        " 🌌 Theme Background Settings ",
                        vec![
                            Line::from(Span::styled("Theme Background Fill (btop style)", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(vec![
                                Span::raw("Current Style: "),
                                if app.theme == ThemeName::System {
                                    Span::styled("SYSTEM TRANSPARENT (System theme active)", Style::default().fg(colors.text_dim))
                                } else if app.theme_background {
                                    Span::styled("THEME BG (btop style palette background)", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
                                } else {
                                    Span::styled("SYSTEM TRANSPARENT (Terminal wallpaper visible)", Style::default().fg(colors.warning).add_modifier(Modifier::BOLD))
                                },
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("Modes explained:", Style::default().fg(colors.text))),
                            Line::from(Span::styled(" • THEME BG: Fills window & panels with theme colors (like btop)", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • SYSTEM TRANSPARENT: Keeps your terminal transparency / wallpaper", Style::default().fg(colors.text_dim))),
                            Line::from(""),
                            Line::from(Span::styled("💡 Tip: Select 'System / Terminal' theme if you want native terminal ANSI!", Style::default().fg(colors.secondary))),
                            Line::from(""),
                            Line::from(Span::styled("💡 Press [Enter], [b] or [Left/Right] to toggle background mode.", Style::default().fg(colors.warning))),
                        ],
                    ),
                    3 => (
                        " 📊 CAVA Visualizer Settings ",
                        vec![
                            Line::from(Span::styled("Console-based Audio Visualizer (CAVA)", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(vec![
                                Span::raw("Status: "),
                                if app.cava_enabled {
                                    Span::styled("ENABLED (in Queue pane)", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
                                } else {
                                    Span::styled("DISABLED", Style::default().fg(colors.error).add_modifier(Modifier::BOLD))
                                },
                            ]),
                            Line::from(""),
                            Line::from(if let Some(path) = CavaManager::find_cava_binary() {
                                Span::styled(format!("CAVA binary: {}", path.display()), Style::default().fg(colors.success))
                            } else {
                                Span::styled("⚠️ CAVA not found in PATH or ~/.local/bin/cava", Style::default().fg(colors.warning))
                            }),
                            Line::from(""),
                            Line::from(Span::styled("Real-time audio frequency equalizer in Queue pane:", Style::default().fg(colors.text))),
                            Line::from(vec![
                                Span::styled("  ▲ Upper rows: Peaks / Treble ", Style::default().fg(colors.visualizer_high)),
                            ]),
                            Line::from(vec![
                                Span::styled("  ■ Middle rows: Vocals / Midtones ", Style::default().fg(colors.visualizer_mid)),
                            ]),
                            Line::from(vec![
                                Span::styled("  ▼ Lower rows: Bass / Sub-bass ", Style::default().fg(colors.visualizer_low)),
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("When enabled, the right column splits into Queue (top 55%) and Audio Spectrum (bottom 45%).", Style::default().fg(colors.text_dim))),
                            Line::from(""),
                            Line::from(Span::styled("💡 Press [Enter] or [v] to toggle visualizer.", Style::default().fg(colors.warning))),
                        ],
                    ),
                    4 => (
                        " 🖼️ Cover Art Settings ",
                        vec![
                            Line::from(Span::styled("Download Covers for MPRIS Widget", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(vec![
                                Span::raw("Status: "),
                                if app.download_covers {
                                    Span::styled("ENABLED", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
                                } else {
                                    Span::styled("DISABLED", Style::default().fg(colors.error).add_modifier(Modifier::BOLD))
                                },
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("When enabled, SoundRust automatically downloads high-res (500x500) album art for the playing track to display in:", Style::default().fg(colors.text))),
                            Line::from(""),
                            Line::from(Span::styled(" • KDE Plasma Media Widget", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • GNOME media controls & lockscreen", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • Waybar / Hyprland media modules", Style::default().fg(colors.text_dim))),
                            Line::from(Span::styled(" • Dunst / Mako / SwayNC popups", Style::default().fg(colors.text_dim))),
                            Line::from(""),
                            Line::from(Span::styled("📁 Storage: ~/.cache/sc-player/covers/", Style::default().fg(colors.text_dim))),
                            Line::from(""),
                            Line::from(Span::styled("💡 Press [Enter] to toggle.", Style::default().fg(colors.warning))),
                        ],
                    ),
                    5 => (
                        " 📻 Autoplay Settings ",
                        vec![
                            Line::from(Span::styled("Spotify-style Infinite Autoplay", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(vec![
                                Span::raw("Status: "),
                                if app.autoplay {
                                    Span::styled("ENABLED", Style::default().fg(colors.success).add_modifier(Modifier::BOLD))
                                } else {
                                    Span::styled("DISABLED", Style::default().fg(colors.error).add_modifier(Modifier::BOLD))
                                },
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("When your playlist or queue ends, SoundRust automatically queries SoundCloud recommendations (/related) to queue up similar tracks indefinitely.", Style::default().fg(colors.text))),
                            Line::from(""),
                            Line::from(Span::styled("💡 Press [Enter] or [a] to toggle.", Style::default().fg(colors.warning))),
                        ],
                    ),
                    6 => (
                        " 🔀 Shuffle Settings ",
                        vec![
                            Line::from(Span::styled("Smart Playlist Shuffle", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(vec![
                                Span::raw("Status: "),
                                if app.shuffle {
                                    Span::styled("ENABLED", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD))
                                } else {
                                    Span::styled("DISABLED", Style::default().fg(colors.text_dim))
                                },
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("Shuffles remaining tracks within your active playlist. Autoplay only begins after all tracks from the playlist are played.", Style::default().fg(colors.text))),
                            Line::from(""),
                            Line::from(Span::styled("💡 Press [Enter] or [s] to toggle.", Style::default().fg(colors.warning))),
                        ],
                    ),
                    7 => (
                        " 👤 Account Settings ",
                        vec![
                            Line::from(Span::styled("SoundCloud Account", Style::default().fg(colors.warning).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(if let Some(ref prof) = app.sc.user_profile {
                                format!("Logged in as @{} (ID: {})", prof.username, prof.id)
                            } else {
                                "Currently in Guest Mode (unauthenticated)".to_string()
                            }),
                            Line::from(""),
                            Line::from(Span::styled("Logging in grants access to your personal playlists and likes.", Style::default().fg(colors.text))),
                            Line::from(""),
                            Line::from(Span::styled("💡 Press [Enter] or [Shift+L] to log in / out.", Style::default().fg(colors.warning))),
                        ],
                    ),
                    8 => (
                        " 🗑️ Cache Settings ",
                        vec![
                            Line::from(Span::styled("Purge Cover Art Cache", Style::default().fg(colors.secondary).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(format!("Currently {} cached cover images on disk.", cached_count)),
                            Line::from(""),
                            Line::from(Span::styled("Deletes cached .jpg images from ~/.cache/sc-player/covers/ to free up disk space.", Style::default().fg(colors.text))),
                            Line::from(""),
                            Line::from(Span::styled("💡 Press [Enter] to delete cache.", Style::default().fg(colors.error).add_modifier(Modifier::BOLD))),
                        ],
                    ),
                    _ => (" ⚙️ Details ", vec![]),
                };

                let widget = Paragraph::new(details)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(title)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(colors.border))
                            .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                    );
                f.render_widget(widget, main_chunks[1]);
            } else {
                // Right Pane: Upcoming Queue
                let queue_items: Vec<ListItem> = app
                    .queue
                    .iter()
                    .enumerate()
                    .take(15)
                    .map(|(idx, t)| {
                        let content = Line::from(vec![
                            Span::styled(format!("{}. ", idx + 1), Style::default().fg(colors.text_dim)),
                            Span::styled(truncate_str(&t.title, 22), Style::default().fg(colors.text)),
                        ]);
                        ListItem::new(content)
                    })
                    .collect();

                let queue_title = format!(" 📻 Upcoming Queue ({}) ", app.queue.len());
                let queue_list = List::new(queue_items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(queue_title)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(colors.border))
                            .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() })
                    );

                if app.cava_enabled {
                    let right_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Percentage(55), // Upcoming Queue
                            Constraint::Percentage(45), // Audio Visualizer
                        ])
                        .split(main_chunks[1]);

                    f.render_widget(queue_list, right_chunks[0]);

                    let is_playing = !state.paused && app.current_track.is_some();
                    let vis_title = if is_playing {
                        " 📊 Audio Visualizer "
                    } else {
                        " 📊 Audio Visualizer [Paused] "
                    };
                    let vis_block = Block::default()
                        .borders(Borders::ALL)
                        .title(vis_title)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(if is_playing { colors.border_active } else { colors.border }))
                        .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() });

                    let inner_vis = vis_block.inner(right_chunks[1]);
                    f.render_widget(vis_block, right_chunks[1]);

                    if inner_vis.width >= 6 && inner_vis.height >= 2 {
                        let w = inner_vis.width as usize;
                        let h = inner_vis.height as usize;

                        let (bar_width, gap) = if w >= 36 { (2, 1) } else { (1, 1) };
                        let num_bars = ((w + gap) / (bar_width + gap)).clamp(4, 32);
                        let total_bars_w = num_bars * bar_width + (num_bars - 1) * gap;
                        let left_pad = w.saturating_sub(total_bars_w) / 2;

                        let bars = app.cava.get_bars(num_bars, is_playing, state.position);

                        let show_labels = h >= 5;
                        let spec_h = if show_labels { h - 1 } else { h };
                        let total_levels = spec_h * 8;

                        const BLOCKS: [char; 8] = [' ', ' ', '▂', '▃', '▄', '▅', '▆', '▇'];
                        let mut lines = Vec::with_capacity(h);

                        for r in 0..spec_h {
                            let row_from_bottom = spec_h - 1 - r;
                            let row_ratio = row_from_bottom as f64 / (spec_h as f64 - 1.0).max(1.0);
                            let row_color = if row_ratio >= 0.70 {
                                colors.visualizer_high
                            } else if row_ratio >= 0.35 {
                                colors.visualizer_mid
                            } else {
                                colors.visualizer_low
                            };

                            let mut spans = Vec::new();
                            if left_pad > 0 {
                                spans.push(Span::raw(" ".repeat(left_pad)));
                            }

                            for (i, &raw_val) in bars.iter().enumerate() {
                                let norm = (raw_val as f64 / cava::MAX_CAVA_RANGE as f64).clamp(0.0, 1.0);
                                let bar_level = (norm * total_levels as f64).round() as usize;

                                let lower_threshold = row_from_bottom * 8;
                                let upper_threshold = (row_from_bottom + 1) * 8;

                                if bar_level >= upper_threshold {
                                    spans.push(Span::styled("█".repeat(bar_width), Style::default().fg(row_color)));
                                } else if bar_level <= lower_threshold {
                                    spans.push(Span::raw(" ".repeat(bar_width)));
                                } else {
                                    let rem = bar_level.saturating_sub(lower_threshold);
                                    let ch = BLOCKS[rem.min(7)];
                                    spans.push(Span::styled(ch.to_string().repeat(bar_width), Style::default().fg(row_color)));
                                }

                                if i + 1 < num_bars {
                                    spans.push(Span::raw(" ".repeat(gap)));
                                }
                            }

                            lines.push(Line::from(spans));
                        }

                        if show_labels {
                            let pad_w = total_bars_w.saturating_sub(16) / 2;
                            let label_line = Line::from(vec![
                                Span::raw(" ".repeat(left_pad)),
                                Span::styled("Bass", Style::default().fg(colors.visualizer_low)),
                                Span::raw(" ".repeat(pad_w.max(2))),
                                Span::styled("Mids", Style::default().fg(colors.visualizer_mid)),
                                Span::raw(" ".repeat(pad_w.max(2))),
                                Span::styled("Treble", Style::default().fg(colors.visualizer_high)),
                            ]);
                            lines.push(label_line);
                        }

                        let p = Paragraph::new(lines)
                            .style(if bg_widget_color != Color::Reset { Style::default().bg(bg_widget_color) } else { Style::default() });
                        f.render_widget(p, inner_vis);
                    }
                } else {
                    f.render_widget(queue_list, main_chunks[1]);
                }
            }

            // 3. Now Playing Bar
            let track_info = if let Some(ref t) = app.current_track {
                let status_icon = if state.paused { "⏸ Paused" } else { "▶ Playing" };
                format!("{} | {} - {}", status_icon, t.user.username, t.title)
            } else {
                "⏹ No track currently playing".to_string()
            };

            let percent = if state.duration > 0.0 {
                (state.position / state.duration * 100.0).clamp(0.0, 100.0) as u16
            } else {
                0
            };

            let time_str = format!("{} / {}", format_duration(state.position), format_duration(state.duration));

            let mut outer_block = Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", track_info))
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(colors.border_active));
            if bg_widget_color != Color::Reset {
                outer_block = outer_block.style(Style::default().bg(bg_widget_color));
            }

            let gauge = Gauge::default()
                .block(outer_block)
                .gauge_style(Style::default().fg(colors.gauge_fg).bg(colors.gauge_bg))
                .percent(percent)
                .label(Span::styled(time_str, Style::default().fg(colors.text).add_modifier(Modifier::BOLD)));
            f.render_widget(gauge, chunks[2]);

            // 4. Footer controls help
            let mut footer_spans = vec![
                Span::styled(" [Tab/1-4]", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                Span::raw(" Tabs "),
                Span::styled("[Enter]", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                Span::raw(" Play "),
                Span::styled("[f]", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD)),
                Span::raw(" Fav "),
                Span::styled("[→/m]", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD)),
                Span::raw(" Menu "),
            ];

            if app.active_tab == ActiveTab::Search {
                footer_spans.push(Span::styled("[c]", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)));
                footer_spans.push(Span::raw(" Filter "));
            }

            footer_spans.extend(vec![
                Span::styled("[t]", Style::default().fg(colors.secondary).add_modifier(Modifier::BOLD)),
                Span::raw(" Theme "),
                Span::styled("[b]", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                Span::raw(" BG "),
                Span::styled("[v]", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                Span::raw(" Visualizer "),
                Span::styled("[s]", Style::default().fg(colors.accent).add_modifier(Modifier::BOLD)),
                Span::raw(" Shuffle "),
                Span::styled("[a]", Style::default().fg(colors.success).add_modifier(Modifier::BOLD)),
                Span::raw(" Autoplay "),
                Span::styled("[Space]", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                Span::raw(" Pause "),
                Span::styled("[n]", Style::default().fg(colors.primary).add_modifier(Modifier::BOLD)),
                Span::raw(" Next "),
                Span::styled("[Shift+L]", Style::default().fg(colors.warning).add_modifier(Modifier::BOLD)),
                Span::raw(" Account "),
                Span::styled("[q]", Style::default().fg(colors.error).add_modifier(Modifier::BOLD)),
                Span::raw(" Quit "),
                Span::styled(format!(" | {}", app.status_message), Style::default().fg(colors.warning)),
            ]);

            let footer = Paragraph::new(Line::from(footer_spans))
                .alignment(Alignment::Left)
                .style(if bg_color != Color::Reset { Style::default().bg(bg_color) } else { Style::default() });

            f.render_widget(footer, chunks[3]);

            // 5. Floating Track Context Menu Modal
            if let Some(ref menu) = app.track_menu {
                let is_fav = app.is_favorite(menu.track.id);
                let menu_items: [(&str, &str); 6] = [
                    ("▶", "Play Now"),
                    ("⏭", "Play Next"),
                    ("➕", "Add to Queue"),
                    if is_fav { ("🤍", "Remove from Favorites") } else { ("❤️", "Add to Favorites") },
                    ("📻", "Start Station"),
                    ("📁", "Add to Playlist"),
                ];

                let popup_width = if menu.playlist_sub_menu {
                    54.min(f.area().width.saturating_sub(4))
                } else {
                    40.min(f.area().width.saturating_sub(4))
                };
                let popup_height = if menu.playlist_sub_menu {
                    (app.user_playlists.len() as u16 + 4).min(16).min(f.area().height.saturating_sub(4))
                } else {
                    (menu_items.len() as u16 + 2).min(f.area().height.saturating_sub(4))
                };
                let x = f.area().x + (f.area().width.saturating_sub(popup_width)) / 2;
                let y = f.area().y + (f.area().height.saturating_sub(popup_height)) / 2;
                let popup_area = Rect { x, y, width: popup_width, height: popup_height };

                f.render_widget(Clear, popup_area);

                let modal_bg = bg_widget_color;
                let block_style = if modal_bg != Color::Reset {
                    Style::default().bg(modal_bg)
                } else {
                    Style::default()
                };

                let inner_width = (popup_width as usize).saturating_sub(4);

                if menu.playlist_sub_menu {
                    let items: Vec<ListItem> = app.user_playlists.iter().enumerate().map(|(idx, pl)| {
                        let is_sel = idx == menu.playlist_selected;
                        let prefix = if is_sel { "❯ " } else { "  " };
                        let line_str = format!("{}{}. {} ({} tracks)", prefix, idx + 1, truncate_str(&pl.title, 22), pl.track_count);
                        let padded = format!("{:<width$}", line_str, width = inner_width);
                        let style = if is_sel {
                            Style::default().bg(colors.highlight_bg).fg(colors.highlight_fg).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(colors.text)
                        };
                        ListItem::new(Span::styled(padded, style))
                    }).collect();

                    let list = List::new(items)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(format!(" 📁 Add to Playlist: '{}' ", truncate_str(&menu.track.title, 20)))
                                .border_type(BorderType::Rounded)
                                .border_style(Style::default().fg(colors.accent))
                                .style(block_style)
                        );
                    f.render_widget(list, popup_area);
                } else {
                    let items: Vec<ListItem> = menu_items.iter().enumerate().map(|(idx, &(icon, label))| {
                        let is_sel = idx == menu.selected;
                        let prefix = if is_sel { "❯ " } else { "  " };
                        let line_str = format!("{}{:<2} {}", prefix, icon, label);
                        let padded = format!("{:<width$}", line_str, width = inner_width);
                        let style = if is_sel {
                            Style::default().bg(colors.highlight_bg).fg(colors.highlight_fg).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(colors.text)
                        };
                        ListItem::new(Span::styled(padded, style))
                    }).collect();

                    let list = List::new(items)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(format!(" 🎵 Actions: '{}' ", truncate_str(&menu.track.title, 20)))
                                .border_type(BorderType::Rounded)
                                .border_style(Style::default().fg(colors.primary))
                                .style(block_style)
                        );
                    f.render_widget(list, popup_area);
                }
            }

            // 6. Floating Playlist Context Menu Modal
            if let Some(ref menu) = app.playlist_menu {
                let popup_width = 44.min(f.area().width.saturating_sub(4));
                let popup_height = (PLAYLIST_MENU_ITEMS.len() as u16 + 2).min(f.area().height.saturating_sub(4));
                let x = f.area().x + (f.area().width.saturating_sub(popup_width)) / 2;
                let y = f.area().y + (f.area().height.saturating_sub(popup_height)) / 2;
                let popup_area = Rect { x, y, width: popup_width, height: popup_height };

                f.render_widget(Clear, popup_area);

                let modal_bg = bg_widget_color;
                let block_style = if modal_bg != Color::Reset {
                    Style::default().bg(modal_bg)
                } else {
                    Style::default()
                };

                let inner_width = (popup_width as usize).saturating_sub(4);

                let items: Vec<ListItem> = PLAYLIST_MENU_ITEMS.iter().enumerate().map(|(idx, &(icon, label))| {
                    let is_sel = idx == menu.selected;
                    let prefix = if is_sel { "❯ " } else { "  " };
                    let line_str = format!("{}{:<2} {}", prefix, icon, label);
                    let padded = format!("{:<width$}", line_str, width = inner_width);
                    let style = if is_sel {
                        Style::default().bg(colors.highlight_bg).fg(colors.highlight_fg).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(colors.text)
                    };
                    ListItem::new(Span::styled(padded, style))
                }).collect();

                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!(" 📁 Playlist: '{}' ", truncate_str(&menu.playlist.title, 20)))
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(colors.accent))
                            .style(block_style)
                    );
                f.render_widget(list, popup_area);
            }
        })?;

        let refresh_dur = if app.cava_enabled && !state.paused && app.current_track.is_some() {
            Duration::from_millis(40)
        } else {
            Duration::from_millis(200)
        };

        tokio::select! {
            _ = tokio::time::sleep(refresh_dur) => {}
            Some(_) = event_rx.recv() => {
                app.next_track().await;
            }
            Some(action) = mpris_rx.recv() => {
                match action {
                    MprisAction::PlayPause => {
                        let _ = app.player.toggle_pause().await;
                    }
                    MprisAction::Next => {
                        app.next_track().await;
                    }
                    MprisAction::Previous => {
                        let _ = app.player.seek(-5.0).await;
                    }
                    MprisAction::Stop => {
                        let _ = app.player.stop().await;
                        app.current_track = None;
                        app.active_playlist = None;
                        if let Some(ref m) = app.mpris {
                            m.set_stopped().await;
                        }
                        app.status_message = "Playback stopped.".to_string();
                    }
                    MprisAction::Seek(delta) => {
                        let _ = app.player.seek(delta).await;
                    }
                    MprisAction::SetVolume(vol) => {
                        let vol = vol.clamp(0.0, 100.0);
                        let _ = app.player.set_volume(vol).await;
                        app.save_volume(vol);
                    }
                    MprisAction::ToggleShuffle => {
                        app.toggle_shuffle();
                    }
                }
            }
            key_opt = key_rx.recv() => {
                match key_opt {
                    Some(key) => {
                        if handle_key(&mut app, key).await {
                            break;
                        }
                    }
                    None => {
                        // Terminal closed / stdin reached EOF
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        }
    }

    let final_vol = app.player.state.read().await.volume;
    app.save_volume(final_vol);
    app.cava.stop().await;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

async fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    let state_volume = app.player.state.read().await.volume;

    // Handle playlist actions context menu when open
    if app.playlist_menu.is_some() {
        match key.code {
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('q') => {
                app.close_playlist_menu();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(ref mut m) = app.playlist_menu {
                    m.prev();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref mut m) = app.playlist_menu {
                    m.next();
                }
            }
            KeyCode::Enter => {
                app.execute_playlist_menu_action().await;
            }
            _ => {}
        }
        return false;
    }

    // Handle track actions context menu when open
    if app.track_menu.is_some() {
        match key.code {
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('q') => {
                let is_submenu = app.track_menu.as_ref().map_or(false, |m| m.playlist_sub_menu);
                if is_submenu {
                    if let Some(ref mut m) = app.track_menu {
                        m.playlist_sub_menu = false;
                    }
                } else {
                    app.close_track_menu();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let is_submenu = app.track_menu.as_ref().map_or(false, |m| m.playlist_sub_menu);
                if is_submenu {
                    let len = app.user_playlists.len();
                    if let Some(ref mut m) = app.track_menu {
                        m.playlist_prev(len);
                    }
                } else if let Some(ref mut m) = app.track_menu {
                    m.prev();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let is_submenu = app.track_menu.as_ref().map_or(false, |m| m.playlist_sub_menu);
                if is_submenu {
                    let len = app.user_playlists.len();
                    if let Some(ref mut m) = app.track_menu {
                        m.playlist_next(len);
                    }
                } else if let Some(ref mut m) = app.track_menu {
                    m.next();
                }
            }
            KeyCode::Enter => {
                app.execute_track_menu_action().await;
            }
            _ => {}
        }
        return false;
    }

    match app.input_mode {
        InputMode::Searching => match key.code {
            KeyCode::Enter => {
                app.input_mode = InputMode::Normal;
                app.execute_search().await;
            }
            KeyCode::Char(c) => {
                app.search_query.push(c);
            }
            KeyCode::Backspace => {
                app.search_query.pop();
            }
            KeyCode::Esc | KeyCode::Down => {
                app.input_mode = InputMode::Normal;
            }
            KeyCode::Tab => {
                app.cycle_search_filter().await;
            }
            KeyCode::BackTab => {
                app.prev_search_filter().await;
            }
            _ => {}
        },
        InputMode::Normal => match key.code {
            KeyCode::Char('q') => {
                return true;
            }
            KeyCode::Tab => {
                app.active_tab = match app.active_tab {
                    ActiveTab::Playlists => ActiveTab::Favorites,
                    ActiveTab::Favorites => ActiveTab::Search,
                    ActiveTab::Search => ActiveTab::Settings,
                    ActiveTab::Settings => ActiveTab::Playlists,
                };
            }
            KeyCode::BackTab => {
                app.active_tab = match app.active_tab {
                    ActiveTab::Playlists => ActiveTab::Settings,
                    ActiveTab::Favorites => ActiveTab::Playlists,
                    ActiveTab::Search => ActiveTab::Favorites,
                    ActiveTab::Settings => ActiveTab::Search,
                };
            }
            KeyCode::Char('1') => {
                app.active_tab = ActiveTab::Playlists;
            }
            KeyCode::Char('2') => {
                app.active_tab = ActiveTab::Favorites;
            }
            KeyCode::Char('3') => {
                app.active_tab = ActiveTab::Search;
                if app.search_results.is_empty() {
                    app.input_mode = InputMode::Searching;
                }
            }
            KeyCode::Char('4') | KeyCode::Char('o') => {
                app.active_tab = ActiveTab::Settings;
            }
            KeyCode::Char('/') => {
                app.active_tab = ActiveTab::Search;
                app.input_mode = InputMode::Searching;
                app.search_query.clear();
            }
            KeyCode::Char('i') if app.active_tab == ActiveTab::Search => {
                app.input_mode = InputMode::Searching;
            }
            KeyCode::Char('c') if app.active_tab == ActiveTab::Search => {
                app.cycle_search_filter().await;
            }
            KeyCode::Char('C') if app.active_tab == ActiveTab::Search => {
                app.prev_search_filter().await;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.select_next();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.select_prev();
            }
            KeyCode::Esc | KeyCode::Backspace => {
                if app.active_tab == ActiveTab::Playlists && app.view_state == ViewState::PlaylistDetail {
                    app.view_state = ViewState::PlaylistList;
                    app.status_message = "Back to playlists list".to_string();
                } else if app.active_tab == ActiveTab::Search && app.search_view_state == SearchViewState::PlaylistDetail {
                    app.search_view_state = SearchViewState::Results;
                    app.status_message = "Back to search results".to_string();
                } else if app.active_tab != ActiveTab::Playlists {
                    app.active_tab = ActiveTab::Playlists;
                }
            }
            KeyCode::Char('f') => {
                match app.active_tab {
                    ActiveTab::Search => {
                        match app.search_view_state {
                            SearchViewState::Results => {
                                if let Some(i) = app.search_list_state.selected() {
                                    if let Some(SearchResultItem::Track(track)) = app.search_results.get(i).cloned() {
                                        app.toggle_favorite(track);
                                    }
                                }
                            }
                            SearchViewState::PlaylistDetail => {
                                if let Some(i) = app.search_playlist_track_list_state.selected() {
                                    if let Some(track) = app.search_playlist_tracks.get(i).cloned() {
                                        app.toggle_favorite(track);
                                    }
                                }
                            }
                        }
                    }
                    ActiveTab::Playlists => {
                        if app.view_state == ViewState::PlaylistDetail {
                            if let Some(i) = app.track_list_state.selected() {
                                if let Some(track) = app.playlist_tracks.get(i).cloned() {
                                    app.toggle_favorite(track);
                                }
                            }
                        }
                    }
                    ActiveTab::Favorites => {
                        if let Some(i) = app.favorites_list_state.selected() {
                            app.remove_favorite_at(i);
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if app.active_tab == ActiveTab::Favorites {
                    if let Some(i) = app.favorites_list_state.selected() {
                        app.remove_favorite_at(i);
                    }
                }
            }
            KeyCode::Char('r') => {
                if app.active_tab == ActiveTab::Favorites {
                    app.sync_soundcloud_likes().await;
                } else if app.active_tab == ActiveTab::Playlists {
                    app.refresh_playlists().await;
                }
            }
            KeyCode::Enter => {
                match app.active_tab {
                    ActiveTab::Settings => {
                        app.toggle_setting().await;
                    }
                    ActiveTab::Favorites => {
                        if let Some(i) = app.favorites_list_state.selected() {
                            if let Some(track) = app.favorites.get(i).cloned() {
                                app.selected_playlist_title = "❤️ Favorites".to_string();
                                app.active_playlist = Some(PlaylistContext {
                                    title: "❤️ Favorites".to_string(),
                                    original_tracks: app.favorites.clone(),
                                });
                                app.queue.clear();
                                for t in app.favorites.iter().skip(i + 1).cloned() {
                                    app.queue.push_back(t);
                                }
                                app.play_track(track).await;
                            }
                        }
                    }
                    ActiveTab::Search => {
                        match app.search_view_state {
                            SearchViewState::Results => {
                                if let Some(i) = app.search_list_state.selected() {
                                    if let Some(item) = app.search_results.get(i).cloned() {
                                        match item {
                                            SearchResultItem::Track(track) => {
                                                app.queue.clear();
                                                app.play_track(track).await;
                                            }
                                            SearchResultItem::Playlist(pl) => {
                                                app.open_search_playlist(pl).await;
                                            }
                                        }
                                    }
                                }
                            }
                            SearchViewState::PlaylistDetail => {
                                if let Some(i) = app.search_playlist_track_list_state.selected() {
                                    app.play_search_playlist_track(i).await;
                                }
                            }
                        }
                    }
                    ActiveTab::Playlists => {
                        match app.view_state {
                            ViewState::PlaylistList => {
                                if let Some(i) = app.playlist_list_state.selected() {
                                    if let Some(pl) = app.user_playlists.get(i).cloned() {
                                        app.open_playlist(pl).await;
                                    }
                                }
                            }
                            ViewState::PlaylistDetail => {
                                if let Some(i) = app.track_list_state.selected() {
                                    app.play_playlist_track(i).await;
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Char('p') => {
                if app.active_tab == ActiveTab::Favorites {
                    if !app.favorites.is_empty() {
                        app.selected_playlist_title = "❤️ Favorites".to_string();
                        app.active_playlist = Some(PlaylistContext {
                            title: "❤️ Favorites".to_string(),
                            original_tracks: app.favorites.clone(),
                        });
                        app.queue.clear();
                        for t in app.favorites.iter().skip(1).cloned() {
                            app.queue.push_back(t);
                        }
                        let first = app.favorites[0].clone();
                        app.play_track(first).await;
                    }
                } else if app.active_tab == ActiveTab::Playlists {
                    match app.view_state {
                        ViewState::PlaylistList => {
                            if let Some(i) = app.playlist_list_state.selected() {
                                if let Some(pl) = app.user_playlists.get(i).cloned() {
                                    app.open_playlist(pl).await;
                                    app.play_entire_playlist().await;
                                }
                            }
                        }
                        ViewState::PlaylistDetail => {
                            app.play_entire_playlist().await;
                        }
                    }
                } else if app.active_tab == ActiveTab::Search {
                    match app.search_view_state {
                        SearchViewState::Results => {
                            if let Some(i) = app.search_list_state.selected() {
                                if let Some(item) = app.search_results.get(i).cloned() {
                                    match item {
                                        SearchResultItem::Playlist(pl) => {
                                            app.play_entire_playlist_from_search(pl, false).await;
                                        }
                                        SearchResultItem::Track(track) => {
                                            app.queue.clear();
                                            app.play_track(track).await;
                                        }
                                    }
                                }
                            }
                        }
                        SearchViewState::PlaylistDetail => {
                            app.play_entire_search_playlist_tracks(false).await;
                        }
                    }
                }
            }
            KeyCode::Char('s') => {
                if app.active_tab == ActiveTab::Favorites && !app.favorites.is_empty() {
                    app.selected_playlist_title = "❤️ Favorites (Shuffle)".to_string();
                    let mut shuffled = app.favorites.clone();
                    shuffle_slice(&mut shuffled);
                    let first = shuffled.remove(0);
                    app.active_playlist = Some(PlaylistContext {
                        title: "❤️ Favorites".to_string(),
                        original_tracks: app.favorites.clone(),
                    });
                    app.queue.clear();
                    for t in shuffled {
                        app.queue.push_back(t);
                    }
                    app.play_track(first).await;
                } else if app.active_tab == ActiveTab::Search {
                    match app.search_view_state {
                        SearchViewState::Results => {
                            if let Some(i) = app.search_list_state.selected() {
                                if let Some(item) = app.search_results.get(i).cloned() {
                                    match item {
                                        SearchResultItem::Playlist(pl) => {
                                            app.play_entire_playlist_from_search(pl, true).await;
                                        }
                                        SearchResultItem::Track(_) => {
                                            app.toggle_shuffle();
                                        }
                                    }
                                } else {
                                    app.toggle_shuffle();
                                }
                            } else {
                                app.toggle_shuffle();
                            }
                        }
                        SearchViewState::PlaylistDetail => {
                            app.play_entire_search_playlist_tracks(true).await;
                        }
                    }
                } else {
                    app.toggle_shuffle();
                }
            }
            KeyCode::Char(' ') => {
                let _ = app.player.toggle_pause().await;
            }
            KeyCode::Char('n') => {
                app.next_track().await;
            }
            KeyCode::Char('a') => {
                app.autoplay = !app.autoplay;
                let mut config = soundcloud::SoundCloud::load_config();
                config.autoplay = app.autoplay;
                let _ = soundcloud::SoundCloud::save_config(&config);
                app.status_message = format!(
                    "📻 Autoplay is now {}",
                    if app.autoplay { "ON (infinite similar music!)" } else { "OFF" }
                );
            }
            KeyCode::Char('t') => {
                app.cycle_theme();
            }
            KeyCode::Char('T') => {
                app.prev_theme();
            }
            KeyCode::Char('b') => {
                app.toggle_theme_background();
            }
            KeyCode::Char('v') => {
                app.toggle_cava().await;
            }
            KeyCode::Char('m') => {
                if app.active_tab == ActiveTab::Search {
                    match app.search_view_state {
                        SearchViewState::Results => {
                            if let Some(i) = app.search_list_state.selected() {
                                if let Some(item) = app.search_results.get(i).cloned() {
                                    match item {
                                        SearchResultItem::Track(track) => {
                                            app.open_track_menu(track);
                                        }
                                        SearchResultItem::Playlist(playlist) => {
                                            app.open_playlist_menu(playlist);
                                        }
                                    }
                                }
                            }
                        }
                        SearchViewState::PlaylistDetail => {
                            if let Some(i) = app.search_playlist_track_list_state.selected() {
                                if let Some(track) = app.search_playlist_tracks.get(i).cloned() {
                                    app.open_track_menu(track);
                                }
                            }
                        }
                    }
                } else if app.active_tab == ActiveTab::Favorites {
                    if let Some(i) = app.favorites_list_state.selected() {
                        if let Some(track) = app.favorites.get(i).cloned() {
                            app.open_track_menu(track);
                        }
                    }
                } else if app.active_tab == ActiveTab::Playlists && app.view_state == ViewState::PlaylistDetail {
                    if let Some(i) = app.track_list_state.selected() {
                        if let Some(track) = app.playlist_tracks.get(i).cloned() {
                            app.open_track_menu(track);
                        }
                    }
                }
            }
            KeyCode::Right => {
                if app.active_tab == ActiveTab::Settings {
                    let selected = app.settings_list_state.selected().unwrap_or(0);
                    if selected == 0 {
                        app.toggle_theme_mode();
                    } else if selected == 1 {
                        app.cycle_preset_theme();
                    } else if selected == 2 {
                        app.toggle_theme_background();
                    } else {
                        app.toggle_setting().await;
                    }
                } else if app.active_tab == ActiveTab::Search {
                    match app.search_view_state {
                        SearchViewState::Results => {
                            if let Some(i) = app.search_list_state.selected() {
                                if let Some(item) = app.search_results.get(i).cloned() {
                                    match item {
                                        SearchResultItem::Track(track) => {
                                            app.open_track_menu(track);
                                        }
                                        SearchResultItem::Playlist(playlist) => {
                                            app.open_playlist_menu(playlist);
                                        }
                                    }
                                }
                            }
                        }
                        SearchViewState::PlaylistDetail => {
                            if let Some(i) = app.search_playlist_track_list_state.selected() {
                                if let Some(track) = app.search_playlist_tracks.get(i).cloned() {
                                    app.open_track_menu(track);
                                }
                            }
                        }
                    }
                } else if app.active_tab == ActiveTab::Favorites {
                    if let Some(i) = app.favorites_list_state.selected() {
                        if let Some(track) = app.favorites.get(i).cloned() {
                            app.open_track_menu(track);
                        }
                    }
                } else if app.active_tab == ActiveTab::Playlists {
                    match app.view_state {
                        ViewState::PlaylistList => {
                            if let Some(i) = app.playlist_list_state.selected() {
                                if let Some(pl) = app.user_playlists.get(i).cloned() {
                                    app.open_playlist(pl).await;
                                }
                            }
                        }
                        ViewState::PlaylistDetail => {
                            if let Some(i) = app.track_list_state.selected() {
                                if let Some(track) = app.playlist_tracks.get(i).cloned() {
                                    app.open_track_menu(track);
                                }
                            }
                        }
                    }
                } else {
                    let _ = app.player.seek(5.0).await;
                }
            }
            KeyCode::Left => {
                if app.active_tab == ActiveTab::Settings {
                    let selected = app.settings_list_state.selected().unwrap_or(0);
                    if selected == 0 {
                        app.toggle_theme_mode();
                    } else if selected == 1 {
                        app.prev_preset_theme();
                    } else if selected == 2 {
                        app.toggle_theme_background();
                    } else {
                        app.toggle_setting().await;
                    }
                } else {
                    let _ = app.player.seek(-5.0).await;
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let vol = (state_volume + 5.0).clamp(0.0, 100.0);
                let _ = app.player.set_volume(vol).await;
                app.save_volume(vol);
            }
            KeyCode::Char('-') => {
                let vol = (state_volume - 5.0).clamp(0.0, 100.0);
                let _ = app.player.set_volume(vol).await;
                app.save_volume(vol);
            }
            KeyCode::Char('L') => {
                app.toggle_account().await;
            }
            _ => {}
        },
    }
    false
}
