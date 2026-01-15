# Mew

Mew is a minimalist, native video wallpaper engine for Windows designed to be lightweight and unobtrusive. I started this project because I wanted a way to play live wallpapers without the overhead of web-based engines that often use significant system resources.

Built with Rust, Direct3D 11, and Slint, Mew aims to feel like a natural part of the desktop experience rather than a separate, resource-heavy application.

---

### Efficiency and Design
Many live wallpaper applications rely on hidden browser instances. Mew takes a more direct approach:
- **Native D3D11 Pipeline**: I've implemented a rendering pipeline using triple-buffering and non-blocking presentation to help keep the Windows shell responsive.
- **Dedicated Threading**: Window management runs on its own thread to minimize interaction delays, such as when right-clicking the desktop or opening icons.
- **Low Resource Usage**: By using native UI components and efficient texture streaming, I aim to keep the memory footprint as small as possible.
- **Power Management**: Mew can automatically pause when you're on battery power or when other apps are in fullscreen to help conserve energy.

---

### Technical Details
- **Logic**: Rust
- **UI**: [Slint](https://slint.dev/) (GPU-accelerated, native interface)
- **Decoding**: FFmpeg (supports hardware acceleration via D3D11VA)
- **Integration**: Deep WorkerW integration for a seamless desktop experience.

---

### Current Progress
The core engine is in a stable state. I recently reached a point where rendering is decoupled from the main Windows message loop, which helps the interface feel much smoother.

- [x] Multi-threaded architectural isolation
- [x] Hardware-accelerated 4K/8K playback
- [x] Clean sidebar-based UI
- [x] Battery and power-state monitoring

---

### Building from Source
Mew is open-source and I welcome anyone who wants to tinker with it. You will need the Rust toolchain and FFmpeg 7.x shared libraries.

1. **Clone the repository**:
   ```powershell
   git clone https://github.com/shubham-pathak1/mew.git
   cd mew
   ```
2. **Setup Environment**: 
   Ensure `$env:FFMPEG_DIR` points to your FFmpeg root directory.
3. **Build**:
   ```powershell
   cargo build --release
   ```
4. **Run**:
   The binary will be located at `./target/release/mew.exe`.

---

### Contributing and Support
I maintain this project individually in my spare time. If you find it useful, a star on the repository is always appreciated.

If you encounter any issues:
- **Check Configuration**: Most issues relate to GPU drivers or FFmpeg paths.
- **Open an Issue**: If you find a bug or a performance bottleneck, please open an issue and I'll do my best to look into it.

*Built with care for a cleaner desktop experience.*