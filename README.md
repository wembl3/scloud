# 🎵 SoundRust v1.0

> A blazingly fast, lightweight SoundCloud terminal player written in Rust.  
> Listen to SoundCloud without Chromium, Electron, or browser tabs eating your RAM.

![Version](https://img.shields.io/badge/version-v1.0.0-green?style=flat-square)
![Rust](https://img.shields.io/badge/rust-2024_edition-orange?logo=rust&style=flat-square)
![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)
![Platform](https://img.shields.io/badge/platform-Linux-lightgrey?logo=linux&style=flat-square)

---

## ✨ Features

- ⚡ **Zero Bloat:** Pure terminal UI powered by [Ratatui](https://github.com/ratatui/ratatui) and [crossterm](https://github.com/crossterm-rs/crossterm). Audio played via an optimized [mpv](https://mpv.io) backend.
- 📻 **Spotify-style Autoplay:** When your playlist or track finishes, the player automatically queries SoundCloud's recommendation algorithms (`/related`) to queue up similar tracks infinitely.
- 📁 **Personal Playlists & Library:** Log into your SoundCloud account to browse, search, and stream your personal playlists.
- 🔀 **Smart Playlist Shuffle:** Shuffles tracks within your playlist first. Autoplay only starts when all songs from your playlist have been heard. Turning shuffle off restores the original track ordering.
- 🖼️ **Album Art in MPRIS Desktop Widgets:** Automatically caches high-res (500x500) album artwork and feeds it to Linux desktop media widgets (KDE Plasma, GNOME, Waybar, Hyprland, Dunst, lockscreen) via `mpris:artUrl`.
- ⚙️ **Interactive Settings Menu:** Built-in settings screen (`[3] ⚙️ Settings` or press `3`/`o`) to easily toggle cover art downloading, autoplay, shuffle, manage accounts, and purge cache.
- 🐧 **Full Linux MPRIS v2 Desktop Integration:** Works out of the box with `playerctl`, media keys, KDE Plasma widgets, GNOME media controls, and lockscreen players.
- 🔍 **Instant Search:** Find and stream any song, artist, or remix across all of SoundCloud in real-time.
- 👤 **Guest Mode & Easy Auth:** Works out of the box in Guest mode without signing in. Login with 1 command (`sc-player login`) or auto-detect your session from Firefox.

---

## 🛠️ Prerequisites

Make sure `mpv` is installed on your system (it provides the lightweight audio engine):

```bash
# Arch Linux
sudo pacman -S mpv

# Debian / Ubuntu
sudo apt install mpv

# Fedora
sudo dnf install mpv
```

---

## 🚀 Installation

Clone the repository and build with Cargo:

```bash
git clone https://github.com/wembl3/scloud.git
cd scloud
cargo build --release

# Symlink or copy to your local bin path
cp target/release/sc_player ~/.local/bin/sc-player
```

Make sure `~/.local/bin` is in your `$PATH`.

---

## 🔐 Account Authentication

You can use `sc-player` without logging in (Guest mode), but signing in allows you to access your personal playlists and library.

### Automatic Detection (Firefox)
If you are already logged into SoundCloud in Firefox, `sc-player` can automatically detect your session:
```bash
sc-player login
```
Press `y` when prompted to use your browser session.

### Manual Token Login
1. Open [soundcloud.com](https://soundcloud.com) in your browser and sign in.
2. Press `F12` to open Developer Tools -> go to **Storage** (or **Application**) -> **Cookies** -> `https://soundcloud.com`.
3. Copy the value of the `oauth_token` cookie.
4. Run:
```bash
sc-player login <YOUR_OAUTH_TOKEN>
```

To log out and return to Guest mode:
```bash
sc-player logout
```

To check your current login status:
```bash
sc-player status
```

---

## 🎮 Keybindings & Controls

| Key | Action |
|---|---|
| `Tab` / `1`, `2`, `3` | Switch between **[1] 📁 Playlists**, **[2] 🔍 Search**, and **[3] ⚙️ Settings** |
| `3` / `o` | Quickly open Settings menu |
| `/` | Focus global search bar |
| `Enter` | Select playlist / track, or toggle option in Settings |
| `p` | Play entire playlist in queue |
| `s` | Toggle **Shuffle** (randomize upcoming playlist tracks) |
| `a` | Toggle **Autoplay** (Spotify-like infinite radio for similar music) |
| `Space` | Play / Pause |
| `n` | Skip to next track |
| `←` / `→` | Seek backwards / forwards (5 seconds) |
| `+` / `-` | Increase / decrease volume |
| `Shift + L` | Account sign in / sign out |
| `Esc` / `Backspace` | Return to playlist list (from playlist detail) |
| `q` | Quit player |

---

## 🐧 MPRIS Media Control (playerctl)

`sc-player` implements the standard Linux MPRIS D-Bus interface (`org.mpris.MediaPlayer2.SoundCloud`):

```bash
# Toggle play/pause
playerctl -p SoundCloud play-pause

# Skip to next track
playerctl -p SoundCloud next

# Show current track metadata
playerctl -p SoundCloud metadata
```

All standard keyboard multimedia keys (Play, Pause, Next, Prev, Stop) and system volume controls work natively.

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).
