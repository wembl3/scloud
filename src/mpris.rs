use mpris_server::{
    zbus::{fdo, Result},
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property, RootInterface,
    Server, Time, TrackId, Volume,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone)]
pub enum MprisAction {
    PlayPause,
    Next,
    Previous,
    Stop,
    Seek(f64),
    SetVolume(f64),
    ToggleShuffle,
}

#[derive(Clone)]
pub struct MprisSharedState {
    pub title: Arc<RwLock<String>>,
    pub artist: Arc<RwLock<String>>,
    pub duration_sec: Arc<RwLock<f64>>,
    pub position_sec: Arc<RwLock<f64>>,
    pub is_paused: Arc<AtomicBool>,
    pub is_playing: Arc<AtomicBool>,
    pub shuffle: Arc<AtomicBool>,
    pub volume: Arc<RwLock<f64>>,
    pub art_url: Arc<RwLock<Option<String>>>,
}

pub struct MprisHandler {
    action_tx: mpsc::Sender<MprisAction>,
    state: MprisSharedState,
}

impl MprisHandler {
    pub fn new(action_tx: mpsc::Sender<MprisAction>, state: MprisSharedState) -> Self {
        Self { action_tx, state }
    }
}

impl RootInterface for MprisHandler {
    async fn raise(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        let _ = self.action_tx.send(MprisAction::Stop).await;
        Ok(())
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_fullscreen(&self, _fullscreen: bool) -> Result<()> {
        Ok(())
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok("SoundRust".to_string())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("sc-player".to_string())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["http".to_string(), "https".to_string()])
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["audio/mpeg".to_string(), "audio/ogg".to_string(), "audio/mp4".to_string()])
    }
}

impl PlayerInterface for MprisHandler {
    async fn next(&self) -> fdo::Result<()> {
        let _ = self.action_tx.send(MprisAction::Next).await;
        Ok(())
    }

    async fn previous(&self) -> fdo::Result<()> {
        let _ = self.action_tx.send(MprisAction::Previous).await;
        Ok(())
    }

    async fn pause(&self) -> fdo::Result<()> {
        let _ = self.action_tx.send(MprisAction::PlayPause).await;
        Ok(())
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        let _ = self.action_tx.send(MprisAction::PlayPause).await;
        Ok(())
    }

    async fn stop(&self) -> fdo::Result<()> {
        let _ = self.action_tx.send(MprisAction::Stop).await;
        Ok(())
    }

    async fn play(&self) -> fdo::Result<()> {
        let _ = self.action_tx.send(MprisAction::PlayPause).await;
        Ok(())
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let secs = offset.as_micros() as f64 / 1_000_000.0;
        let _ = self.action_tx.send(MprisAction::Seek(secs)).await;
        Ok(())
    }

    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        let secs = position.as_micros() as f64 / 1_000_000.0;
        let _ = self.action_tx.send(MprisAction::Seek(secs)).await;
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        if !self.state.is_playing.load(Ordering::SeqCst) {
            Ok(PlaybackStatus::Stopped)
        } else if self.state.is_paused.load(Ordering::SeqCst) {
            Ok(PlaybackStatus::Paused)
        } else {
            Ok(PlaybackStatus::Playing)
        }
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::None)
    }

    async fn set_loop_status(&self, _loop_status: LoopStatus) -> Result<()> {
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn set_rate(&self, _rate: PlaybackRate) -> Result<()> {
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(self.state.shuffle.load(Ordering::SeqCst))
    }

    async fn set_shuffle(&self, _shuffle: bool) -> Result<()> {
        let _ = self.action_tx.send(MprisAction::ToggleShuffle).await;
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let title = self.state.title.read().await.clone();
        let artist = self.state.artist.read().await.clone();
        let dur = *self.state.duration_sec.read().await;
        let art_url = self.state.art_url.read().await.clone();

        let mut builder = Metadata::builder()
            .title(title)
            .artist([artist])
            .length(Time::from_micros((dur * 1_000_000.0) as i64));

        if let Some(ref art) = art_url {
            builder = builder.art_url(art.as_str());
        }

        let meta = builder.build();

        Ok(meta)
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        let vol = *self.state.volume.read().await;
        Ok(vol / 100.0)
    }

    async fn set_volume(&self, volume: Volume) -> Result<()> {
        let _ = self.action_tx.send(MprisAction::SetVolume(volume * 100.0)).await;
        Ok(())
    }

    async fn position(&self) -> fdo::Result<Time> {
        let pos = *self.state.position_sec.read().await;
        Ok(Time::from_micros((pos * 1_000_000.0) as i64))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}

pub struct MprisManager {
    pub server: Arc<Server<MprisHandler>>,
    pub state: MprisSharedState,
}

impl MprisManager {
    pub async fn start(action_tx: mpsc::Sender<MprisAction>) -> Result<Self> {
        let state = MprisSharedState {
            title: Arc::new(RwLock::new("No track".to_string())),
            artist: Arc::new(RwLock::new("SoundCloud".to_string())),
            duration_sec: Arc::new(RwLock::new(0.0)),
            position_sec: Arc::new(RwLock::new(0.0)),
            is_paused: Arc::new(AtomicBool::new(false)),
            is_playing: Arc::new(AtomicBool::new(false)),
            shuffle: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(RwLock::new(85.0)),
            art_url: Arc::new(RwLock::new(None)),
        };

        let handler = MprisHandler::new(action_tx, state.clone());
        let server = Server::new("SoundCloud", handler).await?;
        let server = Arc::new(server);

        Ok(Self { server, state })
    }

    pub async fn update_track(&self, title: &str, artist: &str, duration_sec: f64, art_url: Option<&str>) {
        *self.state.title.write().await = title.to_string();
        *self.state.artist.write().await = artist.to_string();
        *self.state.duration_sec.write().await = duration_sec;
        *self.state.art_url.write().await = art_url.map(|s| s.to_string());
        self.state.is_playing.store(true, Ordering::SeqCst);
        self.state.is_paused.store(false, Ordering::SeqCst);

        let mut builder = Metadata::builder()
            .title(title)
            .artist([artist])
            .length(Time::from_micros((duration_sec * 1_000_000.0) as i64));

        if let Some(art) = art_url {
            builder = builder.art_url(art);
        }

        let meta = builder.build();

        let _ = self.server.properties_changed([
            Property::Metadata(meta),
            Property::PlaybackStatus(PlaybackStatus::Playing),
        ]).await;
    }

    pub async fn update_art_url(&self, art_url: Option<&str>) {
        *self.state.art_url.write().await = art_url.map(|s| s.to_string());
        let title = self.state.title.read().await.clone();
        let artist = self.state.artist.read().await.clone();
        let duration_sec = *self.state.duration_sec.read().await;

        let mut builder = Metadata::builder()
            .title(title)
            .artist([artist])
            .length(Time::from_micros((duration_sec * 1_000_000.0) as i64));

        if let Some(art) = art_url {
            builder = builder.art_url(art);
        }

        let meta = builder.build();

        let _ = self.server.properties_changed([
            Property::Metadata(meta),
        ]).await;
    }

    pub async fn update_playback_state(&self, is_paused: bool, position_sec: f64, volume: f64) {
        let prev_paused = self.state.is_paused.swap(is_paused, Ordering::SeqCst);
        *self.state.position_sec.write().await = position_sec;
        let prev_vol = *self.state.volume.read().await;
        *self.state.volume.write().await = volume;

        // Only send D-Bus signal when pause status or volume actually changes
        if prev_paused != is_paused || (prev_vol - volume).abs() > 1.0 {
            let status = if !self.state.is_playing.load(Ordering::SeqCst) {
                PlaybackStatus::Stopped
            } else if is_paused {
                PlaybackStatus::Paused
            } else {
                PlaybackStatus::Playing
            };

            let _ = self.server.properties_changed([
                Property::PlaybackStatus(status),
                Property::Volume(volume / 100.0),
            ]).await;
        }
    }

    pub async fn set_stopped(&self) {
        self.state.is_playing.store(false, Ordering::SeqCst);
        self.state.is_paused.store(false, Ordering::SeqCst);
        *self.state.position_sec.write().await = 0.0;
        *self.state.art_url.write().await = None;
        let _ = self.server.properties_changed([
            Property::PlaybackStatus(PlaybackStatus::Stopped),
        ]).await;
    }

    pub async fn set_shuffle(&self, shuffle: bool) {
        self.state.shuffle.store(shuffle, Ordering::SeqCst);
        let _ = self.server.properties_changed([
            Property::Shuffle(shuffle),
        ]).await;
    }
}
