mod mpris;
mod player;
mod soundcloud;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use mpris::{MprisAction, MprisManager};
use player::Player;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph},
    Terminal,
};
use soundcloud::{Playlist, SoundCloud, Track};
use std::collections::{HashSet, VecDeque};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(PartialEq, Clone, Copy)]
enum ActiveTab {
    Playlists,
    Search,
}

#[derive(PartialEq)]
enum ViewState {
    PlaylistList,
    PlaylistDetail,
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

struct App {
    sc: SoundCloud,
    player: Player,
    mpris: Option<Arc<MprisManager>>,
    active_tab: ActiveTab,
    view_state: ViewState,
    search_query: String,
    search_results: Vec<Track>,
    user_playlists: Vec<Playlist>,
    selected_playlist_title: String,
    playlist_tracks: Vec<Track>,
    active_playlist: Option<PlaylistContext>,
    search_list_state: ListState,
    playlist_list_state: ListState,
    track_list_state: ListState,
    current_track: Option<Track>,
    queue: VecDeque<Track>,
    history_ids: HashSet<u64>,
    autoplay: bool,
    shuffle: bool,
    input_mode: InputMode,
    status_message: String,
    is_loading: bool,
}

impl App {
    async fn new(event_tx: mpsc::Sender<()>, mpris: Option<Arc<MprisManager>>) -> Result<Self> {
        let sc = SoundCloud::new().await;
        let player = Player::new(event_tx).await?;

        let mut search_list_state = ListState::default();
        search_list_state.select(Some(0));

        let mut playlist_list_state = ListState::default();
        playlist_list_state.select(Some(0));

        let mut track_list_state = ListState::default();
        track_list_state.select(Some(0));

        let mut app = Self {
            sc,
            player,
            mpris,
            active_tab: ActiveTab::Playlists,
            view_state: ViewState::PlaylistList,
            search_query: String::new(),
            search_results: Vec::new(),
            user_playlists: Vec::new(),
            selected_playlist_title: String::new(),
            playlist_tracks: Vec::new(),
            active_playlist: None,
            search_list_state,
            playlist_list_state,
            track_list_state,
            current_track: None,
            queue: VecDeque::new(),
            history_ids: HashSet::new(),
            autoplay: true,
            shuffle: false,
            input_mode: InputMode::Normal,
            status_message: String::new(),
            is_loading: false,
        };

        if let Some(ref prof) = app.sc.user_profile {
            app.status_message = format!("Welcome back, {}! Loading your playlists...", prof.username);
            app.refresh_playlists().await;
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

                    // Update Linux MPRIS system media controls
                    if let Some(ref m) = self.mpris {
                        m.update_track(&track.title, &track.user.username, track.duration as f64 / 1000.0).await;
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
        self.status_message = format!("Searching for '{}'...", query);

        match self.sc.search_tracks(&query, 25).await {
            Ok(results) => {
                self.status_message = format!("Found {} tracks for '{}'", results.len(), query);
                self.search_results = results;
                self.search_list_state.select(Some(0));
            }
            Err(e) => {
                self.status_message = format!("Search failed: {}", e);
            }
        }
        self.is_loading = false;
        self.input_mode = InputMode::Normal;
    }

    fn select_next(&mut self) {
        match self.active_tab {
            ActiveTab::Search => {
                let len = self.search_results.len();
                if len > 0 {
                    let i = self.search_list_state.selected().map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 });
                    self.search_list_state.select(Some(i));
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
            ActiveTab::Search => {
                let len = self.search_results.len();
                if len > 0 {
                    let i = self.search_list_state.selected().map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
                    self.search_list_state.select(Some(i));
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
    println!("SoundRust (sc-player) - Fast SoundCloud Terminal Player\n");
    println!("USAGE:");
    println!("  sc-player               Launch the TUI music player");
    println!("  sc-player login [TOKEN] Authenticate with your SoundCloud account");
    println!("  sc-player logout        Log out and reset to Guest mode");
    println!("  sc-player status        Check authentication status and current user");
    println!("  sc-player help          Print this help message\n");
    println!("CONTROLS IN PLAYER:");
    println!("  1 / 2, Tab    Switch between My Playlists and Search");
    println!("  /             Search tracks globally");
    println!("  Enter         Play selected track / Open playlist");
    println!("  p             Play entire playlist");
    println!("  s             Toggle Shuffle (randomizes playlist & queue)");
    println!("  a             Toggle Spotify-style Autoplay (infinite related tracks)");
    println!("  Space         Pause / Play");
    println!("  n             Next track");
    println!("  Left / Right  Seek -5s / +5s");
    println!("  + / -         Volume up / down");
    println!("  Shift+L       Account Login / Logout");
    println!("  q             Quit");
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

    // Initialize MPRIS D-Bus Service for system controls and playerctl
    let mpris = match MprisManager::start(mpris_tx).await {
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

    let mut render_interval = tokio::time::interval(Duration::from_millis(250));
    render_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        let state = app.player.state.read().await.clone();

        // Sync state to system MPRIS
        if let Some(ref m) = app.mpris {
            m.update_playback_state(state.paused, state.position, state.volume).await;
        }

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header & Tabs
                    Constraint::Min(8),    // Main content & Queue
                    Constraint::Length(4), // Now playing bar
                    Constraint::Length(1), // Footer hotkeys
                ])
                .split(f.area());

            // 1. Header
            let user_badge = if let Some(ref prof) = app.sc.user_profile {
                Span::styled(format!(" [👤 {}] ", prof.username), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [👤 Guest] ", Style::default().fg(Color::DarkGray))
            };

            let auth_btn = if app.sc.oauth_token.is_some() {
                Span::styled(" [Shift+L] Logout ", Style::default().fg(Color::LightRed))
            } else {
                Span::styled(" [Shift+L] Login ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD))
            };

            let tab_playlists = if app.active_tab == ActiveTab::Playlists {
                Span::styled(" [1] 📁 My Playlists ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [1] 📁 My Playlists ", Style::default().fg(Color::Gray))
            };

            let tab_search = if app.active_tab == ActiveTab::Search {
                Span::styled(" [2] 🔍 Search ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [2] 🔍 Search ", Style::default().fg(Color::Gray))
            };

            let shuffle_badge = if app.shuffle {
                Span::styled(" [🔀 SHUFFLE: ON] ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [SHUFFLE: OFF] ", Style::default().fg(Color::DarkGray))
            };

            let autoplay_badge = if app.autoplay {
                Span::styled(" [📻 AUTOPLAY: ON] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [AUTOPLAY: OFF] ", Style::default().fg(Color::DarkGray))
            };

            let search_prompt = if app.input_mode == InputMode::Searching {
                format!(" Search: {}█ ", app.search_query)
            } else {
                " [/] Search ".to_string()
            };

            let header = Paragraph::new(Line::from(vec![
                Span::styled(" SoundRust ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                user_badge,
                auth_btn,
                tab_playlists,
                Span::raw(" "),
                tab_search,
                shuffle_badge,
                autoplay_badge,
                Span::styled(format!("Vol: {:.0}% ", state.volume), Style::default().fg(Color::Yellow)),
                Span::raw("| "),
                Span::styled(search_prompt, if app.input_mode == InputMode::Searching { Style::default().fg(Color::White).bg(Color::Blue) } else { Style::default().fg(Color::DarkGray) }),
            ]))
            .block(Block::default().borders(Borders::ALL).title(" SoundRust ").border_type(BorderType::Rounded));

            f.render_widget(header, chunks[0]);

            // 2. Middle Content & Queue
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                .split(chunks[1]);

            // Left Pane: depending on active tab
            match app.active_tab {
                ActiveTab::Playlists => {
                    if app.sc.oauth_token.is_none() {
                        let text = vec![
                            Line::from(""),
                            Line::from(Span::styled("   🔒 Guest Mode (Not Authenticated)", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                            Line::from(""),
                            Line::from(Span::styled("   To view and stream your personal SoundCloud playlists:", Style::default().fg(Color::White))),
                            Line::from(""),
                            Line::from(vec![
                                Span::styled("   1. Press ", Style::default().fg(Color::Gray)),
                                Span::styled("[Shift+L]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                                Span::styled(" to auto-login from your local Firefox browser session, OR", Style::default().fg(Color::Gray)),
                            ]),
                            Line::from(vec![
                                Span::styled("   2. Run ", Style::default().fg(Color::Gray)),
                                Span::styled("'sc-player login'", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                                Span::styled(" in any terminal to sign in manually.", Style::default().fg(Color::Gray)),
                            ]),
                            Line::from(""),
                            Line::from(Span::styled("   💡 You can already search and stream ANY track right now via [2] 🔍 Search (or press [/])!", Style::default().fg(Color::Green))),
                        ];
                        let widget = Paragraph::new(text)
                            .block(Block::default().borders(Borders::ALL).title(" 📁 Your Playlists ").border_type(BorderType::Rounded));
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
                                            Span::styled(format!("{:2}. ", idx + 1), Style::default().fg(Color::DarkGray)),
                                            Span::styled(format!("{:<40} ", truncate_str(&p.title, 40)), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                                            Span::styled(format!("({} tracks)", p.track_count), Style::default().fg(Color::Yellow)),
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
                                    .block(Block::default().borders(Borders::ALL).title(title).border_type(BorderType::Rounded))
                                    .highlight_style(Style::default().bg(Color::Rgb(40, 60, 100)).add_modifier(Modifier::BOLD))
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
                                        let content = Line::from(vec![
                                            Span::styled(format!("{:2}. ", idx + 1), Style::default().fg(Color::DarkGray)),
                                            Span::styled(format!("{:<40} ", truncate_str(&t.title, 40)), Style::default().fg(Color::White)),
                                            Span::styled(format!("by {:<18} ", truncate_str(&t.user.username, 18)), Style::default().fg(Color::Yellow)),
                                            Span::styled(dur, Style::default().fg(Color::Cyan)),
                                        ]);
                                        ListItem::new(content)
                                    })
                                    .collect();

                                let title = format!(" 📁 Playlist: '{}' (Press [p] to play whole playlist, [Esc] back) ", app.selected_playlist_title);
                                let list = List::new(items)
                                    .block(Block::default().borders(Borders::ALL).title(title).border_type(BorderType::Rounded))
                                    .highlight_style(Style::default().bg(Color::Rgb(40, 60, 100)).add_modifier(Modifier::BOLD))
                                    .highlight_symbol("▶ ");

                                f.render_stateful_widget(list, main_chunks[0], &mut app.track_list_state);
                            }
                        }
                    }
                }
                ActiveTab::Search => {
                    let items: Vec<ListItem> = app
                        .search_results
                        .iter()
                        .enumerate()
                        .map(|(idx, t)| {
                            let dur = format_duration(t.duration as f64 / 1000.0);
                            let content = Line::from(vec![
                                Span::styled(format!("{:2}. ", idx + 1), Style::default().fg(Color::DarkGray)),
                                Span::styled(format!("{:<40} ", truncate_str(&t.title, 40)), Style::default().fg(Color::White)),
                                Span::styled(format!("by {:<18} ", truncate_str(&t.user.username, 18)), Style::default().fg(Color::Yellow)),
                                Span::styled(dur, Style::default().fg(Color::Cyan)),
                            ]);
                            ListItem::new(content)
                        })
                        .collect();

                    let title = if app.is_loading {
                        " 🔍 Search Results [Loading...] "
                    } else {
                        " 🔍 Search Results "
                    };

                    let list = List::new(items)
                        .block(Block::default().borders(Borders::ALL).title(title).border_type(BorderType::Rounded))
                        .highlight_style(Style::default().bg(Color::Rgb(40, 60, 100)).add_modifier(Modifier::BOLD))
                        .highlight_symbol("▶ ");

                    f.render_stateful_widget(list, main_chunks[0], &mut app.search_list_state);
                }
            }

            // Right Pane: Upcoming Queue
            let queue_items: Vec<ListItem> = app
                .queue
                .iter()
                .enumerate()
                .take(15)
                .map(|(idx, t)| {
                    let content = Line::from(vec![
                        Span::styled(format!("{}. ", idx + 1), Style::default().fg(Color::DarkGray)),
                        Span::styled(truncate_str(&t.title, 22), Style::default().fg(Color::Gray)),
                    ]);
                    ListItem::new(content)
                })
                .collect();

            let queue_title = format!(" 📻 Upcoming Queue ({}) ", app.queue.len());
            let queue_list = List::new(queue_items)
                .block(Block::default().borders(Borders::ALL).title(queue_title).border_type(BorderType::Rounded));

            f.render_widget(queue_list, main_chunks[1]);

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

            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL).title(format!(" {} ", track_info)).border_type(BorderType::Rounded))
                .gauge_style(Style::default().fg(Color::Green).bg(Color::Rgb(30, 30, 30)))
                .percent(percent)
                .label(Span::styled(time_str, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));

            f.render_widget(gauge, chunks[2]);

            // 4. Footer controls help
            let footer = Paragraph::new(Line::from(vec![
                Span::styled(" [Tab/1,2]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Tabs "),
                Span::styled("[Enter]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Play "),
                Span::styled("[p]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Play All "),
                Span::styled("[s]", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                Span::raw(" Shuffle "),
                Span::styled("[a]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(" Autoplay "),
                Span::styled("[Space]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Pause "),
                Span::styled("[n]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Next "),
                Span::styled("[Shift+L]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(" Account "),
                Span::styled("[q]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::raw(" Quit "),
                Span::styled(format!(" | {}", app.status_message), Style::default().fg(Color::Yellow)),
            ]))
            .alignment(Alignment::Left);

            f.render_widget(footer, chunks[3]);
        })?;

        tokio::select! {
            _ = render_interval.tick() => {}
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
                        let _ = app.player.set_volume(vol).await;
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

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

async fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    let state_volume = app.player.state.read().await.volume;

    match app.input_mode {
        InputMode::Searching => match key.code {
            KeyCode::Enter => {
                app.execute_search().await;
            }
            KeyCode::Char(c) => {
                app.search_query.push(c);
            }
            KeyCode::Backspace => {
                app.search_query.pop();
            }
            KeyCode::Esc => {
                app.input_mode = InputMode::Normal;
            }
            _ => {}
        },
        InputMode::Normal => match key.code {
            KeyCode::Char('q') => {
                return true;
            }
            KeyCode::Tab => {
                app.active_tab = match app.active_tab {
                    ActiveTab::Playlists => ActiveTab::Search,
                    ActiveTab::Search => ActiveTab::Playlists,
                };
            }
            KeyCode::Char('1') => {
                app.active_tab = ActiveTab::Playlists;
            }
            KeyCode::Char('2') => {
                app.active_tab = ActiveTab::Search;
            }
            KeyCode::Char('/') => {
                app.input_mode = InputMode::Searching;
                app.search_query.clear();
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
                }
            }
            KeyCode::Enter => {
                match app.active_tab {
                    ActiveTab::Search => {
                        if let Some(i) = app.search_list_state.selected() {
                            if let Some(track) = app.search_results.get(i).cloned() {
                                app.queue.clear();
                                app.play_track(track).await;
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
                if app.active_tab == ActiveTab::Playlists {
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
                }
            }
            KeyCode::Char('s') => {
                app.toggle_shuffle();
            }
            KeyCode::Char(' ') => {
                let _ = app.player.toggle_pause().await;
            }
            KeyCode::Char('n') => {
                app.next_track().await;
            }
            KeyCode::Char('a') => {
                app.autoplay = !app.autoplay;
                app.status_message = format!(
                    "📻 Autoplay is now {}",
                    if app.autoplay { "ON (infinite similar music!)" } else { "OFF" }
                );
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let _ = app.player.seek(5.0).await;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let _ = app.player.seek(-5.0).await;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let vol = state_volume + 5.0;
                let _ = app.player.set_volume(vol).await;
            }
            KeyCode::Char('-') => {
                let vol = state_volume - 5.0;
                let _ = app.player.set_volume(vol).await;
            }
            KeyCode::Char('L') => {
                app.toggle_account().await;
            }
            _ => {}
        },
    }
    false
}
