# OpenCut Desktop

Built with [GPUI](https://www.gpui.rs).

> [!WARNING]
> Very early. Right now this is just a window that opens.

## Running

Rust is pinned in `rust-toolchain.toml` at the repo root and selected by rustup.

```sh
cargo run -p opencut-desktop
cargo check -p opencut-desktop --locked
cargo build -p opencut-desktop --release --locked
```

The first build compiles GPUI from source and takes a while. The root `Cargo.lock` is committed.

## Platform requirements

- **macOS**: Xcode command line tools (Metal renderer).
- **Windows**: no extra dependencies (Win32 + DirectWrite).
- **Linux**: renders via Vulkan (Blade), windows via Wayland or X11 (both enabled by default). System packages (Debian/Ubuntu names): `libvulkan1` + working Vulkan drivers, `libwayland-dev`, `libx11-xcb-dev`, `libxkbcommon-x11-dev`, `libfontconfig-dev`, plus a C toolchain and `cmake`.
- **WSL2/WSLg**: uses XWayland automatically when available. GPUI 0.2.2 requires `xdg_wm_base` v2–5, while WSLg advertises v1.
