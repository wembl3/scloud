use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use std::sync::RwLock as StdRwLock;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::RwLock as TokioRwLock;

pub const DEFAULT_CAVA_BARS: usize = 48;
pub const MAX_CAVA_RANGE: u8 = 100;

pub struct CavaManager {
    bars: Arc<StdRwLock<Vec<u8>>>,
    last_update: Arc<StdRwLock<Instant>>,
    is_running: Arc<AtomicBool>,
    child: Arc<TokioRwLock<Option<Child>>>,
}

impl CavaManager {
    pub fn new() -> Self {
        Self {
            bars: Arc::new(StdRwLock::new(vec![0; DEFAULT_CAVA_BARS])),
            last_update: Arc::new(StdRwLock::new(Instant::now())),
            is_running: Arc::new(AtomicBool::new(false)),
            child: Arc::new(TokioRwLock::new(None)),
        }
    }

    pub fn find_cava_binary() -> Option<PathBuf> {
        let candidates = [
            "/home/surface/.local/bin/cava",
            "/usr/bin/cava",
            "/usr/local/bin/cava",
        ];
        for path in &candidates {
            let p = PathBuf::from(path);
            if p.is_file() {
                return Some(p);
            }
        }
        // Check PATH
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                let candidate = dir.join("cava");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn write_config() -> Result<PathBuf> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let config_dir = PathBuf::from(home).join(".config").join("sc-player");
        fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join("cava.conf");
        let config_content = format!(
            "[general]\n\
             bars = {}\n\
             framerate = 30\n\
             autosens = 1\n\n\
             [input]\n\
             method = pulse\n\n\
             [output]\n\
             method = raw\n\
             raw_target = /dev/stdout\n\
             data_format = ascii\n\
             ascii_max_range = {}\n\
             bar_delimiter = 59\n",
            DEFAULT_CAVA_BARS, MAX_CAVA_RANGE
        );

        fs::write(&config_path, config_content)?;
        Ok(config_path)
    }

    pub async fn start(&self) {
        let cava_bin = match Self::find_cava_binary() {
            Some(bin) => bin,
            None => return,
        };

        let config_path = match Self::write_config() {
            Ok(p) => p,
            Err(_) => return,
        };

        // Terminate any previous child
        self.stop().await;

        let mut cmd = Command::new(cava_bin);
        cmd.arg("-p")
            .arg(config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(_) => return,
        };

        let stdout = match child.stdout.take() {
            Some(out) => out,
            None => return,
        };

        *self.child.write().await = Some(child);
        self.is_running.store(true, Ordering::SeqCst);

        let bars = self.bars.clone();
        let last_update = self.last_update.clone();
        let is_running = self.is_running.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while is_running.load(Ordering::SeqCst) {
                match reader.next_line().await {
                    Ok(Some(line)) => {
                        let parsed: Vec<u8> = line
                            .split(';')
                            .filter_map(|s| s.trim().parse::<u8>().ok())
                            .map(|v| v.min(MAX_CAVA_RANGE))
                            .collect();

                        if !parsed.is_empty() {
                            if let Ok(mut b) = bars.write() {
                                *b = parsed;
                            }
                            if let Ok(mut lu) = last_update.write() {
                                *lu = Instant::now();
                            }
                        }
                    }
                    Ok(None) => break, // EOF
                    Err(_) => break,
                }
            }
            is_running.store(false, Ordering::SeqCst);
        });
    }

    pub async fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Some(mut child) = self.child.write().await.take() {
            let _ = child.kill().await;
        }
    }

    pub fn get_bars(&self, target_count: usize, is_playing: bool, position_sec: f64) -> Vec<u8> {
        if target_count == 0 {
            return Vec::new();
        }

        if !is_playing {
            // Decay bars to 0 when paused or stopped
            if let Ok(mut current) = self.bars.write() {
                for v in current.iter_mut() {
                    *v = v.saturating_sub(2);
                }
            }
            return vec![0; target_count];
        }

        let elapsed = self.last_update.read().map(|l| l.elapsed().as_millis()).unwrap_or(9999);
        let raw = self.bars.read().map(|b| b.clone()).unwrap_or_default();
        let has_live_cava_signal = elapsed < 800 && raw.iter().any(|&v| v > 0);

        if has_live_cava_signal {
            // Resample raw CAVA bars to target_count
            let mut res = Vec::with_capacity(target_count);
            for i in 0..target_count {
                let idx = (i * raw.len()) / target_count;
                res.push(raw[idx.min(raw.len() - 1)]);
            }
            res
        } else {
            // Organic fallback dynamic wave when cava has silence or is starting up
            let mut res = Vec::with_capacity(target_count);
            let t = position_sec * 6.0;
            for i in 0..target_count {
                let x = i as f64 * 0.25;
                let wave1 = ((x + t).sin() + 1.0) * 0.5;
                let wave2 = ((x * 1.7 - t * 1.3).cos() + 1.0) * 0.5;
                let wave3 = (((x + 2.0) * 0.8 + t * 2.0).sin() + 1.0) * 0.5;
                let combined = (wave1 * 0.4 + wave2 * 0.4 + wave3 * 0.2).clamp(0.0, 1.0);
                let height = (combined * MAX_CAVA_RANGE as f64).round() as u8;
                res.push(height);
            }
            res
        }
    }

    #[allow(dead_code)]
    pub fn get_braille_wave(&self, char_width: usize, is_playing: bool, position_sec: f64) -> String {
        if char_width == 0 {
            return String::new();
        }
        let col_count = char_width * 2;
        let bars = self.get_bars(col_count, is_playing, position_sec);
        let mut s = String::with_capacity(char_width);
        for i in 0..char_width {
            let idx = i * 2;
            let b1 = if idx < bars.len() { ((bars[idx] as u32 * 4) / MAX_CAVA_RANGE as u32) as u8 } else { 0 };
            let b2 = if idx + 1 < bars.len() { ((bars[idx + 1] as u32 * 4) / MAX_CAVA_RANGE as u32) as u8 } else { 0 };
            s.push(braille_char(b1.min(4), b2.min(4)));
        }
        s
    }
}

#[allow(dead_code)]
pub fn braille_char(h1: u8, h2: u8) -> char {
    const COL1: [u32; 5] = [0, 0x40, 0x44, 0x46, 0x47];
    const COL2: [u32; 5] = [0, 0x80, 0xA0, 0xB0, 0xB8];
    let v1 = COL1[(h1 as usize).min(4)];
    let v2 = COL2[(h2 as usize).min(4)];
    std::char::from_u32(0x2800 + v1 + v2).unwrap_or(' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cava_binary_found() {
        let bin = CavaManager::find_cava_binary();
        assert!(bin.is_some(), "cava binary should be found on this system");
    }

    #[test]
    fn test_cava_config_generation() {
        let config = CavaManager::write_config().unwrap();
        assert!(config.exists());
        let content = std::fs::read_to_string(config).unwrap();
        assert!(content.contains("bars = 48"));
        assert!(content.contains("ascii_max_range = 100"));
    }

    #[test]
    fn test_get_bars_bounds() {
        let manager = CavaManager::new();
        let bars_zero = manager.get_bars(0, true, 10.0);
        assert_eq!(bars_zero.len(), 0);

        let bars_paused = manager.get_bars(32, false, 10.0);
        assert_eq!(bars_paused.len(), 32);
        assert!(bars_paused.iter().all(|&v| v == 0));

        let bars_playing = manager.get_bars(64, true, 5.5);
        assert_eq!(bars_playing.len(), 64);
        for &val in &bars_playing {
            assert!(val <= MAX_CAVA_RANGE, "Bar value {} exceeded max range {}", val, MAX_CAVA_RANGE);
        }
    }

    #[test]
    fn test_get_braille_wave() {
        let manager = CavaManager::new();
        let wave = manager.get_braille_wave(20, true, 2.0);
        assert_eq!(wave.chars().count(), 20);
        let empty_wave = manager.get_braille_wave(0, true, 2.0);
        assert_eq!(empty_wave.len(), 0);
    }
}
