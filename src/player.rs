use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Default)]
pub struct PlaybackState {
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub idle: bool,
    pub volume: f64,
}

#[derive(Debug)]
pub enum PlayerCommand {
    Load(String),
    TogglePause,
    Seek(f64),
    SetVolume(f64),
    Stop,
    QueryStatus,
}

#[derive(Debug, Deserialize)]
struct MpvResponse {
    pub event: Option<String>,
    pub reason: Option<String>,
    pub request_id: Option<u64>,
    pub data: Option<serde_json::Value>,
}

pub struct Player {
    command_tx: mpsc::Sender<PlayerCommand>,
    pub state: Arc<RwLock<PlaybackState>>,
    pub is_running: Arc<AtomicBool>,
    socket_path: String,
    _process: Child,
}

impl Drop for Player {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = self._process.kill();
    }
}

impl Player {
    pub async fn new(event_tx: mpsc::Sender<()>, initial_volume: f64) -> Result<Self> {
        let initial_vol = initial_volume.clamp(0.0, 100.0);
        let socket_path = format!("/tmp/sc_player_mpv_{}.sock", std::process::id());
        let _ = std::fs::remove_file(&socket_path);

        // Start MPV with PipeWire/Pulse/ALSA audio and smooth buffer
        let process = Command::new("mpv")
            .arg("--no-video")
            .arg("--idle=yes")
            .arg(format!("--input-ipc-server={}", socket_path))
            .arg("--ao=pipewire,pulse,alsa")
            .arg(format!("--volume={:.1}", initial_vol))
            .arg("--cache=yes")
            .arg("--demuxer-max-bytes=50M")
            .arg("--demuxer-readahead-secs=30")
            .arg("--audio-buffer=0.2")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn mpv process. Ensure mpv is installed.")?;

        // Wait for socket to be created
        let mut connected = false;
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if std::path::Path::new(&socket_path).exists() {
                connected = true;
                break;
            }
        }
        if !connected {
            anyhow::bail!("MPV IPC socket was not created in time");
        }

        // Connect a single bidirectional Unix stream
        let stream = UnixStream::connect(&socket_path).await?;
        let (reader, mut writer) = stream.into_split();

        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PlayerCommand>(64);
        let state = Arc::new(RwLock::new(PlaybackState {
            volume: initial_vol,
            idle: true,
            ..Default::default()
        }));
        let is_running = Arc::new(AtomicBool::new(true));

        // Writer task: handles all outgoing commands through the single socket
        let is_running_writer = is_running.clone();
        tokio::spawn(async move {
            while is_running_writer.load(Ordering::SeqCst) {
                if let Some(cmd) = cmd_rx.recv().await {
                    let cmd_json = match cmd {
                        PlayerCommand::Load(url) => {
                            let val = serde_json::json!({
                                "command": ["loadfile", url, "replace"]
                            });
                            format!("{}\n", val)
                        }
                        PlayerCommand::TogglePause => {
                            "{\"command\": [\"cycle\", \"pause\"]}\n".to_string()
                        }
                        PlayerCommand::Seek(delta) => {
                            let val = serde_json::json!({
                                "command": ["seek", delta, "relative"]
                            });
                            format!("{}\n", val)
                        }
                        PlayerCommand::SetVolume(vol) => {
                            let val = serde_json::json!({
                                "command": ["set_property", "volume", vol]
                            });
                            format!("{}\n", val)
                        }
                        PlayerCommand::Stop => {
                            "{\"command\": [\"stop\"]}\n".to_string()
                        }
                        PlayerCommand::QueryStatus => {
                            "{\"command\": [\"get_property\", \"time-pos\"], \"request_id\": 1}\n\
                             {\"command\": [\"get_property\", \"duration\"], \"request_id\": 2}\n\
                             {\"command\": [\"get_property\", \"pause\"], \"request_id\": 3}\n\
                             {\"command\": [\"get_property\", \"idle-active\"], \"request_id\": 4}\n"
                                .to_string()
                        }
                    };

                    if writer.write_all(cmd_json.as_bytes()).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Reader task: parses responses and events from the socket
        let state_clone = state.clone();
        let is_running_reader = is_running.clone();
        tokio::spawn(async move {
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            while is_running_reader.load(Ordering::SeqCst) {
                line.clear();
                if let Ok(n) = buf_reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    if let Ok(resp) = serde_json::from_str::<MpvResponse>(&line) {
                        // End of track detection
                        if let Some(ev_name) = &resp.event {
                            if ev_name == "end-file" {
                                if let Some(reason) = &resp.reason {
                                    if reason == "eof" {
                                        let _ = event_tx.send(()).await;
                                    }
                                }
                            }
                        }

                        // Property query responses
                        if let Some(req_id) = resp.request_id {
                            if let Some(data) = resp.data {
                                let mut st = state_clone.write().await;
                                match req_id {
                                    1 => {
                                        if let Some(pos) = data.as_f64() {
                                            st.position = pos;
                                        }
                                    }
                                    2 => {
                                        if let Some(dur) = data.as_f64() {
                                            st.duration = dur;
                                        }
                                    }
                                    3 => {
                                        if let Some(paused) = data.as_bool() {
                                            st.paused = paused;
                                        }
                                    }
                                    4 => {
                                        if let Some(idle) = data.as_bool() {
                                            st.idle = idle;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                } else {
                    break;
                }
            }
        });

        // Periodic poller: requests time-pos, duration, pause, idle every 250ms
        let cmd_tx_poller = cmd_tx.clone();
        let is_running_poller = is_running.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(250));
            while is_running_poller.load(Ordering::SeqCst) {
                interval.tick().await;
                let _ = cmd_tx_poller.send(PlayerCommand::QueryStatus).await;
            }
        });

        Ok(Self {
            command_tx: cmd_tx,
            state,
            is_running,
            socket_path,
            _process: process,
        })
    }

    pub async fn play(&self, url: &str) -> Result<()> {
        self.command_tx.send(PlayerCommand::Load(url.to_string())).await?;
        Ok(())
    }

    pub async fn toggle_pause(&self) -> Result<()> {
        self.command_tx.send(PlayerCommand::TogglePause).await?;
        Ok(())
    }

    pub async fn seek(&self, delta: f64) -> Result<()> {
        self.command_tx.send(PlayerCommand::Seek(delta)).await?;
        Ok(())
    }

    pub async fn set_volume(&self, vol: f64) -> Result<()> {
        self.command_tx.send(PlayerCommand::SetVolume(vol)).await?;
        let mut st = self.state.write().await;
        st.volume = vol.clamp(0.0, 100.0);
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.command_tx.send(PlayerCommand::Stop).await?;
        let mut st = self.state.write().await;
        st.idle = true;
        st.position = 0.0;
        st.duration = 0.0;
        Ok(())
    }
}
