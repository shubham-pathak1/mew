# Mew 🐾

**Mew** is a lightweight, performance-focused animated wallpaper engine for Windows. Built with **Rust** and **Slint**, it aims to provide the essential features of a live wallpaper engine with a significantly lower resource footprint than existing alternatives.

## 🚀 The Goal: Performance First

Most wallpaper engines use heavy web technologies (WebView2/Chromium) or complex runtimes, often consuming 150MB - 300MB of RAM. **Mew's target is <60MB** during active playback.

- **Lightweight**: Native UI and efficient video decoding.
- **Battery-Friendly**: Automatically pauses playback based on system state.
- **Reliable**: Designed to survive Windows Explorer restarts.

## 🛠️ Tech Stack

- **Logic**: [Rust](https://www.rust-lang.org/)
- **UI Framework**: [Slint](https://slint.dev/) (Native, no WebView overhead)
- **Video Decoding**: [FFmpeg 7.0](https://ffmpeg.org/) (Hardware accelerated via `ffmpeg-next`)
- **OS Integration**: [Windows API](https://github.com/microsoft/windows-rs) (GDI rendering, WorkerW integration)

## ✨ Current Status: MVP Scaffolding

Mew is currently in the **Phase 1: MVP** development stage. The core architecture is scaffolded, featuring:

- [x] **Video Decoding**: FFmpeg integration for MP4, WebM, and AVI support.
- [x] **Desktop Rendering**: The "WorkerW" trick to render wallpapers behind desktop icons.
- [x] **Performance Logic**: Battery level monitoring and fullscreen app detection.
- [x] **Settings UI**: A clean, dark-themed Slint interface for configuration.

## 📥 Setup & Building

To build Mew from source, you need the Rust toolchain and FFmpeg 7.0 libraries.

### Prerequisites

1.  **Rust**: [Install Rust](https://www.rust-lang.org/tools/install)
2.  **FFmpeg 7.0**: 
    - Download FFmpeg 7.0 shared libraries for Windows.
    - Set the `FFMPEG_DIR` environment variable to the directory containing `lib` and `include` folders.
    - Add the `bin` folder to your system `PATH`.

### Build

```bash
git clone https://github.com/shubham-pathak1/mew.git
cd mew
cargo build --release
```

## 🤝 Contributing

Mew is open-source and welcomes contributions! Being "honest" about our status: it's early days. We are focusing on making the core video engine rock-solid before adding extras like GIFs or shaders.

If you find a bug or have a performance optimization, please open an issue or a PR.

## 📜 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details (coming soon).

---

*Built with ❤️ for a faster desktop experience.*
