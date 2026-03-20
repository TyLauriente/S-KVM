# Software KVM switch: comprehensive design document

A Rust-based software KVM switch that surpasses Synergy, Barrier, and Input Director is entirely feasible today, with the ecosystem providing mature crates for every subsystem. **Tauri v2 for the GUI, quinn (QUIC) for transport, platform-specific input backends behind a trait abstraction, and an actor-based async architecture form the optimal foundation.** This document covers architecture decisions, protocol design, crate selections with versions, and implementation strategies across all eleven technical domains. The open-source project **lan-mouse** (Rust, v0.10.0) validates this architecture in production and serves as the most important reference implementation.

---

## 1. Architecture and GUI framework selection

### Why Tauri v2 wins for this application

**Tauri v2** (stable at **2.10.3**, March 2026, MIT/Apache-2.0, ~104k GitHub stars) is the recommended GUI framework for three decisive reasons: its **built-in system tray** (`TrayIconBuilder`), its **official global-shortcut plugin** (`tauri-plugin-global-shortcut` ~2.3.x), and the entire web ecosystem available for building the monitor layout editor (Konva.js, Fabric.js, react-dnd). Building an interactive drag-and-drop grid editor in Iced or Slint would require weeks of custom widget development; in Tauri, it leverages mature JavaScript canvas libraries.

Tauri's architecture cleanly separates the **Rust backend** (system-level input capture, networking, daemon management) from the **web frontend** (configuration UI), communicating via JSON-serialized IPC commands (`#[tauri::command]`). The frontend runs on the system webview (WebView2 on Windows, WebKitGTK on Linux) — no bundled Chromium, resulting in **~1–3 MB binaries** and **~30–80 MB RAM**. Multi-window support is built-in via `WebviewWindowBuilder`, with inter-window events via `emit_to()`.

**Wayland caveats exist but are manageable.** Known issues include tray icons not appearing in dev mode on GNOME (#14234), context menu positioning errors (#13608), and some GTK4/WebKitGTK rendering quirks. These are upstream issues under active work. Critically, the input capture layer (the hard Wayland problem) lives in the Rust backend regardless of GUI choice — it uses `libei`/portals and is framework-agnostic.

| Feature | Tauri v2 (2.10) | Iced (0.14) | Slint (1.14) |
|---|---|---|---|
| Monitor grid editor | ★★★★★ (any JS canvas lib) | ★★★ (custom widget required) | ★★ (manual DSL implementation) |
| System tray | Built-in, first-class | External crate, manual wiring | External crate, manual wiring |
| Global hotkeys | Official plugin | External `global-hotkey` crate | External `global-hotkey` crate |
| Wayland support | ★★★ (buggy, improving) | ★★★★★ (powers COSMIC desktop) | ★★★★ (winit-based, solid) |
| API stability | 2.x stable | Pre-1.0, breaking changes | 1.x stable, no breakage |
| Bundle size | ~1–3 MB | ~2–5 MB | ~300 KB–2 MB |

If Tauri's Wayland issues become a dealbreaker, **Iced 0.14** is the fallback — its Wayland support is the strongest in the Rust ecosystem (System76's COSMIC desktop runs entirely on Iced), and its Canvas + PaneGrid widgets provide a foundation for the grid editor, though expect 3–5× more UI development time.

### Recommended project structure

The codebase should separate into a **workspace** with independent crates: `core` (platform input capture, protocol), `daemon` (background service managing connections), `config` (shared configuration types with serde), `src-tauri` (Tauri backend bridging core/daemon with GUI), and `src/` (web frontend with React/Svelte + canvas library for the monitor layout editor). The daemon should be a separate process from the GUI, communicating via Unix sockets or named pipes — this follows the pattern established by Barrier/Deskflow and lan-mouse.

---

## 2. Low-latency input forwarding

### Platform-specific capture is non-negotiable

Cross-platform input crates (`rdev`, `enigo`, `inputbot`) lack Wayland support and production reliability. The proven architecture — demonstrated by **lan-mouse** — uses **platform-specific backends behind a common trait**, selected at runtime. Lan-mouse's `input-capture` and `input-emulation` crates (published separately on crates.io) are reusable reference implementations.

**Windows capture** uses **low-level hooks** (`SetWindowsHookExW` with `WH_KEYBOARD_LL` and `WH_MOUSE_LL`) via the `windows` crate (microsoft/windows-rs, `windows-sys` **v0.61.0**). Hooks are preferred over Raw Input because they can **block input from reaching local applications** by returning a non-zero value from the callback — essential for forwarding mode. The callback must return within Windows' ~300ms timeout (configurable via `LowLevelHooksTimeout` registry key), so events should be serialized into a channel and processed on a separate network thread.

**Windows injection** uses `SendInput` with `KEYEVENTF_SCANCODE` (not virtual keys — scan codes work correctly with DirectInput games) and `MOUSEEVENTF_ABSOLUTE` for absolute positioning with coordinates normalized to 0–65535.

**Linux capture** uses the `evdev` crate (**v0.12+**, pure Rust, no C dependencies) with `device.grab()` (EVIOCGRAB ioctl) for **exclusive access** that prevents events from reaching the compositor. This works on both X11 and Wayland. The crate supports tokio async via the `tokio` feature flag and includes built-in uinput support. Permissions require the `input` group or root.

**Linux injection** uses **uinput** via the same `evdev` crate's `VirtualDeviceBuilder` — creating virtual keyboard and mouse devices that appear as real hardware to the kernel. This is the most reliable injection method across both X11 and Wayland.

**Wayland-specific challenges** require multiple backends:
- **wlroots compositors** (Sway, Hyprland): layer-shell protocol — 1-pixel-wide invisible windows on screen edges detect cursor arrival
- **GNOME ≥45, KDE ≥6.1**: `libei` (Emulated Input) — the `reis` Rust crate provides bindings
- **Fallback**: `org.freedesktop.portal.RemoteDesktop` XDG portal (requires user consent dialog)

### Input forwarding protocol design

The Synergy/Barrier protocol (documented in `ProtocolTypes.h`) uses 4-byte ASCII command codes over TCP: `DKDN` (key down: keyID + modifierMask + button), `DMMV` (absolute mouse move: x, y), `DMRM` (relative mouse move: dx, dy), `CINN` (enter screen: x, y, sequenceNumber, modifierMask), and `DINF` (screen info: dimensions, cursor position). The critical design lesson from Barrier is to **send scan codes (physical key positions), not characters or virtual keys** — this avoids keyboard layout mismatch problems. Modifier state synchronization happens at screen-enter time via a full modifier bitmask.

For serialization, use **`bincode`** or **`postcard`** (compact binary) rather than JSON. An input event struct should contain: event type (enum), scan code or button ID, modifier bitmask, relative/absolute coordinates, and a microsecond timestamp. Target message size: **8–32 bytes per event**.

### Mouse cursor warping across monitors

Edge detection follows the Barrier model: the server continuously monitors cursor position, and when it hits a configured screen edge, sends `COUT` (leave) locally and `CINN` (enter) to the target peer with mapped coordinates. Coordinate transformation scales Y proportionally when monitor resolutions differ: `y_scaled = y × (target_height / source_height)`. For **DPI scaling**, forward physical pixel coordinates and let the receiving OS apply its own scaling. For mouse acceleration, forward **raw relative deltas** and let each OS apply its own acceleration curve — attempting to match acceleration between PCs creates more problems than it solves.

### Transport: QUIC hybrid is optimal

**Quinn** (**v0.11.9**, 86M+ downloads, production-ready) provides the ideal transport through its dual capability: **unreliable datagrams** for mouse movement (fire-and-forget, no retransmission delay, ~UDP semantics) and **reliable ordered streams** for keyboard events, clipboard, and control messages. This hybrid approach within a single QUIC connection eliminates the need for separate UDP sockets while providing built-in TLS 1.3 encryption and 0-RTT reconnection.

| Event type | QUIC primitive | Rationale |
|---|---|---|
| Mouse movement (relative) | Unreliable datagram | Highest frequency, tolerate loss (next event supersedes) |
| Mouse buttons, keyboard | Reliable stream | Must arrive, must be ordered |
| Screen enter/leave | Reliable stream | Critical control messages |
| Clipboard | Reliable stream | Large payloads, must complete |
| Keepalive | Unreliable datagram | Periodic, loss-tolerant |

Lan-mouse achieves **<1ms latency on gigabit LAN** using DTLS-encrypted UDP (`webrtc-dtls` crate). Quinn's QUIC adds ~100–200µs of encryption overhead — negligible for input forwarding.

---

## 3. Remote display streaming

### GPU-resident capture-to-encode pipeline

The critical design principle, stated explicitly by Parsec: **never let the raw frame touch system memory.** The pipeline must be GPU-resident from capture through encoding.

**Windows capture** uses **DXGI Desktop Duplication API** via the `win_desktop_duplication` crate (42k downloads, designed for game-streaming use cases). It returns `ID3D11Texture2D` GPU textures directly with dirty-rectangle metadata. Latency is one vsync interval (~6.9ms at 144Hz). The Windows.Graphics.Capture API is unsuitable due to its mandatory yellow consent border overlay.

**Linux capture** on Wayland uses **PipeWire** via the `lamco-pipewire` crate (**v0.1.4**) with DMA-BUF support, achieving **<2ms frame latency** and <5% CPU at 1080p@60Hz. DMA-BUF avoids GPU→CPU copies entirely by sharing framebuffers via file descriptors that can be passed directly to hardware encoders.

**Hardware encoding** uses `ffmpeg-next` (**v7.1.x**, supports FFmpeg 3.4–8.0, 2.2M+ downloads) for multi-vendor support (`h264_nvenc`, `h264_amf`, `h264_vaapi`, `h264_qsv`). For maximum NVENC control, `nvenc-sys` provides direct FFI bindings. The color conversion from BGRA (capture format) to NV12 (encoder input) must happen on-GPU via a `wgpu` compute shader or CUDA kernel.

Ultra-low-latency encoding settings for NVENC: **preset P1** (fastest), **ultra_low_latency tuning**, **CBR rate control**, **0 B-frames** (reordering adds latency), **0 lookahead** (buffering adds latency), **VBV buffer = bitrate/fps** (one frame), **infinite GOP** with on-demand IDR, and `repeatSPSPPS=true` for stream recovery. **H.264 delivers the lowest encode latency (~1–2ms)**; H.265 offers 30–40% better bitrate efficiency at slightly higher encode cost (~2–3ms).

### Achievable latency budget

Parsec demonstrates **4–8ms glass-to-glass at 240fps on LAN** using their proprietary BUD (Better User Datagrams) protocol with zero-buffer design and direct hardware encoder APIs. Sunshine/Moonlight achieves **10–15ms on LAN** using FFmpeg-based encoding over the reverse-engineered GameStream protocol. For this project, targeting **sub-16ms on LAN** is realistic with the described GPU-resident pipeline.

```
Host:  Capture (~0ms GPU) → Color convert (~0.5ms GPU) → Encode (~1-2ms NVENC) → Network (~0.5ms LAN)
Client: Receive (~0.5ms) → Decode (~1-3ms HW) → Render (~0.5ms GPU blit)
Total: ~4-8ms at high frame rates
```

### Streaming protocol

For video transport, use **quinn QUIC unreliable datagrams** (essentially encrypted UDP without retransmission) or a custom UDP protocol with application-level FEC (Forward Error Correction) for packet loss resilience. WebRTC adds unnecessary SDP negotiation and STUN/TURN overhead when endpoints are known. If browser client support is ever needed, the `str0m` crate (sans-I/O WebRTC, no Arc/Mutex, clean Rust API with RTP-level access) is the best option.

### Virtual monitor creation

**Windows** requires an **IddCx (Indirect Display Driver)** — a user-mode driver (UMDF, not kernel-mode) that creates virtual display outputs. The `IddSampleDriver` project (ge9/IddSampleDriver) and `Virtual-Display-Driver` (VirtualDrivers) support resolutions up to 8K and refresh rates up to 500Hz. Both Parsec and Sunshine use IddCx-based virtual displays. Installation is via Device Manager "Add Legacy Hardware" — no kernel driver signing needed for UMDF.

**Linux** uses **VKMS** (`modprobe vkms`) for a software DRM/KMS device that appears as a real display to compositors, or wlroots' native **headless backend** (`WLR_BACKENDS=drm,libinput,headless`) for adding/removing virtual outputs at runtime.

---

## 4. Bidirectional audio sharing

### Capture and injection stack

Audio capture uses **loopback recording** — capturing the mix of all audio playing on a system. On Windows, **WASAPI** via the `wasapi` crate (**v0.5.0**) supports loopback capture with `AUDCLNT_STREAMFLAGS_LOOPBACK` in shared mode (~10–20ms latency). On Linux, **PipeWire** (`pipewire` crate **v0.9.2**) captures monitor ports of the default sink, or `libpulse-binding` (**v2.x**, 164k monthly downloads) captures PulseAudio monitor sources.

The cross-platform abstraction layer uses **`cpal`** (**v0.16.0**, 8.7M+ downloads) which supports WASAPI loopback on Windows and PipeWire/PulseAudio backends on Linux (via feature flags). For Windows-specific features like process-specific capture, supplement with the native `wasapi` crate.

### Opus encoding at 5ms frames

**Opus** (RFC 6716) is the clear codec choice — royalty-free, **5ms algorithmic delay** in `RESTRICTED_LOWDELAY` mode, 64–128 kbps for stereo at excellent quality. Use the `opus` crate (by Tad Hardesty, 49k monthly downloads) or `audiopus` (**v0.2.0**, 824k total downloads). For KVM over LAN, **5ms or 10ms frame sizes** give <15ms codec delay with good quality. Raw PCM is technically feasible on gigabit LAN (~1.5 Mbps) but Opus provides robustness against jitter with built-in Forward Error Correction.

### Audio routing architecture

Each peer runs bidirectional capture and playback. Audio from the active PC (receiving keyboard/mouse focus) is captured, Opus-encoded, and streamed to the controller for playback. On KVM switch, a short crossfade (~10–20ms) transitions between audio streams. An **adaptive jitter buffer** (10–20ms depth on LAN, expandable for WAN) absorbs network timing variance. Clock drift between PCs requires periodic sample rate adjustment (~1 sample/second correction). Realistic end-to-end latency: **25–30ms** comfortably, **<20ms** with aggressive buffer settings.

---

## 5. FIDO2/passkey forwarding

### Protocol-level CTAP2 relay beats USB/IP

Two approaches exist: full USB/IP device passthrough (Parsec's approach — installs a virtual USB driver, forwards entire USB device) and **protocol-level CTAP2 interception** (Microsoft RDP WebAuthn redirection, Citrix HDX virtual channel). Protocol-level forwarding is superior for a KVM: lower latency, minimal bandwidth (~1KB per auth request), no device exclusivity on the source PC, and the ability to filter/audit commands.

**CTAP2** (Client to Authenticator Protocol 2, spec revision Feb 2025) uses USB HID as transport with 64-byte packets. The key commands are `authenticatorMakeCredential` (0x01) and `authenticatorGetAssertion` (0x02), carrying CBOR-encoded payloads. The forwarding architecture: on the PC needing authentication, create a **virtual FIDO2 HID device** that receives CTAPHID packets from the browser/OS; forward the CBOR payload over the KVM's encrypted network channel; on the PC with the physical key, relay to the real authenticator via `hidapi`; return the signed response.

**Linux virtual FIDO2 device**: use `uhid-virt` crate (**v0.0.8**, 130k+ downloads) to create a virtual HID device via `/dev/uhid` with the FIDO HID report descriptor (usage page 0xF1D0). The `soft-fido2` crate (**v0.4.x**) provides a complete CTAP 2.0/2.1/2.2 implementation with uhid transport — the best foundation for this feature.

**Windows virtual FIDO2 device**: no pure userspace equivalent of uhid exists. Use the **USB/IP approach** — the `usbip` crate (**v0.7.1**) creates a USB/IP server on localhost that emulates a FIDO2 USB device, avoiding kernel driver development entirely. The `virtual-fido` project (Go) demonstrates this pattern working on both platforms.

Key crates: `ctap-hid-fido2` (**v3.5.8**, cross-platform CTAP2 client), `soft-fido2` (protocol implementation + virtual authenticator), `hidapi` (cross-platform HID access), `uhid-virt` (Linux virtual HID), `usbip` (USB/IP device emulation).

**Security consideration**: FIDO2 is designed to resist phishing — network forwarding technically breaks this model. Mitigations include requiring user touch/PIN on every forwarded request (FIDO2 already mandates this), TLS-encrypting the relay channel, logging all forwarded requests, and optionally restricting forwarding to `getAssertion` only (blocking `makeCredential` to prevent credential creation on untrusted hosts).

---

## 6. Networking, discovery, and encryption

### LAN discovery with mDNS-SD

Use **`mdns-sd`** (**v0.13.11**, actively maintained through 2025, 155 GitHub stars, 97% doc coverage) for both service advertising and browsing. It handles multiple network interfaces automatically, works with or without async runtimes via `flume` channels, and is verified compatible with Avahi (Linux) and Bonjour (macOS/Windows). Register a service type like `_softkvm._tcp.local` with TXT records carrying version, machine UUID, display geometry, and OS type.

**mDNS does not work over Tailscale** — confirmed, as Tailscale operates at Layer 3 and does not forward multicast traffic (long-requested feature, unimplemented as of 2025). For WAN peers via Tailscale, use Tailscale's MagicDNS (`hostname.tailnet.ts.net`) or manual peer configuration. ZeroTier (Layer 2 alternative) does support mDNS if needed.

### Full-mesh QUIC topology

For 3–5 PCs, a **full mesh** of QUIC connections is trivially manageable — each peer maintains N−1 connections. Quinn's `Endpoint` acts as both client and server on a single UDP socket. One peer is designated the **active controller** (has keyboard/mouse focus) at any time; controller designation migrates on screen-edge transition or hotkey.

On discovery of a new peer via mDNS, all existing peers initiate QUIC connections to it. On peer departure (connection timeout/close), the peer is removed and others are notified via the control stream. `libp2p` is feature-rich (mDNS, Kademlia DHT, Noise, Gossipsub) but overkill for <10 peers — direct quinn connections are simpler and more controllable.

### Multi-stream QUIC transport architecture

A single QUIC connection between each peer pair carries all data types via multiplexed streams and datagrams:

- **Stream 0** (bidirectional, reliable): Control channel — handshake, screen layout negotiation, capability exchange, heartbeat
- **Stream 1** (unidirectional, reliable): Input events — keyboard, mouse buttons (must be reliable and ordered)
- **Stream 2** (unidirectional, reliable): Clipboard data, FIDO2 relay
- **Unreliable datagrams**: Video frames (loss-tolerant, high-throughput, no HOL blocking)
- **Unreliable datagrams**: Audio packets (low-latency, tolerate small loss)

This design uses a **single firewall port** per peer, shares congestion context across all data types, and provides encryption for everything through QUIC's mandatory TLS 1.3.

### Encryption and authentication

QUIC via quinn uses **`rustls`** (**v0.23.36**) for TLS 1.3 with `aws-lc-rs` or `ring` crypto providers. This is sufficient — adding Noise protocol on top would be redundant double encryption. For peer authentication without a CA, the recommended model is **self-signed certificates + TOFU** (Trust On First Use):

1. Each peer generates a self-signed Ed25519 certificate using **`rcgen`** (maintained by the rustls team)
2. First connection: display SHA-256 fingerprint, user confirms match (like SSH `known_hosts`)
3. Store trusted fingerprints persistently; alert on change (potential MITM)
4. For better UX: implement a **pairing ceremony** — one peer displays a 6-digit code, the other enters it, deriving a pre-shared key for the initial TLS handshake, then exchanging and pinning certificates for future connections

This directly addresses Barrier's 2021 security audit findings, which revealed unauthenticated client acceptance and weak SHA-1 fingerprints. Running QUIC over Tailscale creates double encryption (WireGuard + TLS 1.3) with ~100–200µs overhead per packet — acceptable, and worth keeping for defense-in-depth.

---

## 7. Rhai scripting for user automation

**Rhai** (**v1.24.0**, 5.5M+ downloads, pure Rust, MIT/Apache-2.0) provides an embedded scripting language with JavaScript-like syntax that's purpose-built for Rust integration. Its sandboxing is excellent for a KVM: **no filesystem or network access by default** — scripts can only call APIs you explicitly expose via `engine.register_fn()`. Protection against malicious scripts includes stack overflow prevention, data size limits, and operation count caps (`Engine::set_max_operations()`).

Exposing KVM APIs to Rhai is straightforward: register functions like `switch_to_screen(n)`, `get_peers()`, `send_clipboard(text)`, and custom types via `engine.register_type_with_name::<PeerInfo>("PeerInfo")`. Use cases include custom hotkey actions, conditional monitor switching rules ("when laptop disconnects, move focus to desktop"), scheduled workflows, and per-application input remapping. Performance benchmarks show ~1M iterations in 0.14s — more than sufficient for automation tasks where millisecond-scale response is adequate.

If Lua is preferred by users, **`mlua`** (**v0.11.4**) is 2–5× faster for pure computation and offers familiar syntax, but Rhai's native Rust integration, sandboxing, and zero C dependencies make it the better default.

---

## 8. Event-driven architecture with actors

### Tokio channels as the nervous system

Structure the application around **tokio channels** rather than shared `Arc<Mutex<>>` state:

- **`mpsc`**: Multiple subsystems → central coordinator (e.g., all actors report status to a monitor)
- **`broadcast`**: Config changes, shutdown signals → all subsystems simultaneously
- **`watch`**: Current active screen, connection status — readers always see latest value, no queue buildup
- **`oneshot`**: Request/reply patterns ("get current config" returns a single value)

Use `tokio::select!` to concurrently await multiple event sources without polling. Use `spawn_blocking` for CPU-intensive work (video encoding, Opus encoding) to avoid starving the async runtime.

### Actor frameworks for subsystem management

**Ractor** (**v0.15.10**, 419k downloads, Erlang-inspired) and **Kameo** (**v0.19.2**, best overall comparison score, built-in PubSub) are the top choices. Both run on Tokio and provide supervision (restart crashed actors), bounded mailboxes (backpressure), and clean message-passing APIs.

The actor mapping for KVM subsystems: `InputActor` (captures/translates input), `VideoActor` (capture/encode pipeline), `AudioActor` (audio routing/mixing), `NetworkActor` (QUIC connection management), `PeerActor` (one per connected peer, manages session state), `ConfigActor` (runtime configuration, broadcasts changes), `ScriptActor` (Rhai engine, mediates script↔system calls).

For **graceful shutdown**, use `CancellationToken` from `tokio-util` (propagated hierarchically to all subsystems) or the `tokio-graceful-shutdown` crate which provides a `Toplevel` + `SubsystemHandle` abstraction that catches SIGINT/SIGTERM, manages subsystem lifecycle with timeouts.

---

## 9. Build system and packaging

**Cargo workspace** is the primary build system. CMake integration is needed only for C/C++ dependencies like FFmpeg. The **Corrosion** CMake module (**v0.6**) handles this: `FetchContent_Declare(Corrosion ...)` → `corrosion_import_crate(MANIFEST_PATH rust-lib/Cargo.toml)` → `target_link_libraries(your_target PUBLIC rust-lib)`. It calls `cargo metadata` to discover targets, creates CMake IMPORTED targets, and handles cross-compilation and feature flags.

Tauri v2's CLI (`tauri build`) handles packaging directly: **NSIS/WiX** installers on Windows, **.deb/.rpm/AppImage** on Linux, **.app/.dmg** on macOS. For build orchestration beyond Cargo, prefer **`just`** (simple command runner) over `cargo-make` for task definitions. Cross-compilation uses `cargo-xwin` (Windows targets from Linux/macOS) or the `cross` crate (Docker-based). CI/CD runs a matrix across platforms with `dtolnay/rust-toolchain@stable`, `swatinem/rust-cache@v2`, and `tauri-apps/tauri-action@v0`.

---

## 10. Testing strategy

### Test infrastructure

**`cargo-nextest`** replaces `cargo test` as the test runner — up to **60% faster** with process-per-test isolation, flaky test detection, and CI sharding (`--partition slice:m/n`). Use `#[tokio::test]` for async tests and `tokio::time::pause()` for deterministic time control.

**`mockall`** (**v0.13.1**, 84.9M+ downloads) provides `#[automock]` for trait-based mocking with full async support. Abstract every platform interface behind a trait: `trait InputSource { async fn next_event(&self) -> InputEvent; }`, `trait NetworkTransport { async fn send(&self, data: &[u8]) -> Result<()>; }`. Mock these for unit tests; use real implementations in integration tests over loopback.

**`proptest`** (**v1.10.0**, 93.4M+ downloads) is preferred over quickcheck for property-based testing — more flexible per-value strategies, better automatic shrinking. Key properties to test:

- Protocol message serialization roundtrips: `encode(msg) |> decode == msg`
- Input mapping determinism: same input + config always produces same output
- State machine transitions: peer connection state machine never reaches invalid states
- Screen coordinate mapping: coordinate transform is bijective within valid ranges

For networking tests, use loopback sockets for integration tests and the **`turmoil`** crate for simulating network conditions (latency, packet loss). For video pipeline tests, use small reference frames (64×64 solid color) and verify encode→decode roundtrips.

---

## 11. Security posture

### Layered defense for a system-level tool

Apply **`#[forbid(unsafe_code)]`** in your own crates' `lib.rs`/`main.rs` — this makes any `unsafe` block a compile error in your code while allowing it in vetted dependencies (tokio, windows-rs, ffmpeg bindings). Audit the dependency tree with:

- **`cargo-geiger`**: Counts unsafe usage per dependency
- **`cargo-audit`**: Checks against the RustSec Advisory Database for known CVEs
- **`cargo-deny`**: Policy enforcement for licenses, banned crates, advisory compliance, source restrictions

The safe FFI wrapper pattern isolates unsafe code in dedicated `-sys` or `-ffi` crates: validate all inputs in the safe wrapper, call the FFI function in an `unsafe` block, and convert error codes to `Result<T, E>`.

For peer authentication, implement mutual TLS from day one (Barrier's 2021 audit revealed their server accepted unauthenticated clients — a critical vulnerability). The `rcgen` + `rustls` + `quinn` stack provides certificate generation, pure-Rust TLS 1.3, and encrypted QUIC transport with no OpenSSL dependency. Use SHA-256 fingerprints (Barrier originally used SHA-1, upgraded after audit), persist trusted fingerprints with alerts on change, and consider a pairing ceremony (6-digit code or QR) for user-friendly initial trust establishment.

---

## Complete crate dependency reference

| Subsystem | Crate | Version | Purpose |
|---|---|---|---|
| **GUI** | `tauri` | 2.10.x | Application framework |
| | `tauri-plugin-global-shortcut` | ~2.3 | System-wide hotkeys |
| | `tauri-plugin-single-instance` | ~2.2 | Prevent duplicate instances |
| | `tauri-plugin-store` | ~2.4 | Persistent key-value config |
| **Input (Windows)** | `windows` / `windows-sys` | 0.58+ / 0.61 | Win32 hooks, SendInput, Raw Input |
| **Input (Linux)** | `evdev` | 0.12+ | evdev capture + uinput injection |
| | `reis` | latest | libei bindings for Wayland |
| **Transport** | `quinn` | 0.11.9 | QUIC (streams + unreliable datagrams) |
| | `rustls` | 0.23.36 | TLS 1.3 |
| | `rcgen` | 0.14+ | Self-signed certificate generation |
| **Discovery** | `mdns-sd` | 0.13.11 | mDNS/DNS-SD service advertising/browsing |
| **Video capture** | `win_desktop_duplication` | latest | DXGI DD GPU texture capture |
| | `lamco-pipewire` | 0.1.4 | PipeWire + DMA-BUF screen capture |
| **Video encode** | `ffmpeg-next` | 7.1.x | Hardware-accelerated encoding |
| | `nvenc-sys` | latest | Direct NVENC FFI bindings |
| **GPU compute** | `wgpu` | 25.x | BGRA→NV12 color conversion shader |
| **Audio** | `cpal` | 0.16.0 | Cross-platform audio I/O |
| | `wasapi` | 0.5.0 | Windows WASAPI loopback capture |
| | `pipewire` | 0.9.2 | Linux PipeWire audio |
| | `opus` | latest | Opus codec bindings |
| **FIDO2** | `soft-fido2` | 0.4.x | CTAP2 protocol + virtual authenticator |
| | `ctap-hid-fido2` | 3.5.8 | CTAP2 client for real authenticators |
| | `uhid-virt` | 0.0.8 | Linux virtual HID device |
| | `usbip` | 0.7.1 | USB/IP device emulation (Windows) |
| **Async runtime** | `tokio` | 1.x | Async runtime + channels |
| | `tokio-util` | 0.7.x | CancellationToken, codecs |
| **Actors** | `ractor` | 0.15.10 | Actor framework (or `kameo` 0.19.2) |
| **Scripting** | `rhai` | 1.24.0 | Embedded scripting engine |
| **Serialization** | `serde` + `bincode` | 1.x + 1.x | Binary serialization for input events |
| **Testing** | `mockall` | 0.13.1 | Trait mocking |
| | `proptest` | 1.10.0 | Property-based testing |
| **Security** | `cargo-audit` | latest | CVE scanning |
| | `cargo-geiger` | latest | Unsafe code counting |
| | `cargo-deny` | latest | Supply chain policy |
| **Build** | `corrosion` | 0.6 | CMake↔Rust integration |

## Conclusion: what makes this better than existing tools

The key architectural advantages over Barrier/Synergy are: **Rust's memory safety** eliminating the class of vulnerabilities found in Barrier's 2021 audit; **QUIC transport** providing built-in encryption, stream multiplexing, and 0-RTT reconnection versus Barrier's raw TCP with bolted-on SSL; **Wayland-native input** via libei/portals instead of X11-only capture; and **display streaming** capability that Barrier entirely lacks. The lan-mouse project validates that this architecture works in practice. The additions of virtual monitor creation (IddCx/VKMS), hardware-accelerated video streaming, bidirectional audio, FIDO2 forwarding, and Rhai scripting push beyond what any existing open-source software KVM offers — creating a tool that bridges the gap between Synergy-style input sharing and Parsec-style remote desktop, controlled by a single unified system.