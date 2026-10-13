# Craft App Launcher v0.1.0

A portable, native Windows x64 launcher for Craft applications. Choose your apps folder, launch a selected version, and install verified portable updates side by side. No fixed drive letter or `reCraft` folder is required.

## Requirements

Windows x64 with the Microsoft Visual C++ v14 x64 runtime. If Windows reports a missing `VCRUNTIME140.dll`, install the official [Microsoft Visual C++ Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist). The launcher package does not bundle this runtime.

## Start

1. Extract the release into a writable folder, such as `C:\Tools\CraftLauncher`.
2. Keep `craft-app-launcher.exe`, `craft-launcher-agent.exe`, and `craft-launcher-pair.json` together. These three files are a matched release.
3. Run `craft-app-launcher.exe` and choose the folder containing your portable Craft installations.
4. Click an app tile to launch its GUI. If several versions exist, choose one through the tile's context menu or App settings.

Settings and logs are saved beside the launcher. You can move the launcher and apps together while preserving their relative layout on the same drive. If a saved root becomes unavailable, choose it again in Settings.

## Features

- Twelve app families: CADCraft, DeckCraft, DesignCraft, EffectCraft, FilmCraft, GridCraft, LightCraft, PDFCraft, PhotoCraft, SoundCraft, VectorCraft, and WordCraft.
- Exact GUI/CLI executable-name discovery without running apps to identify them; conservative PE file-version reporting and explicit version selection.
- Named profiles with per-app arguments and per-computer FilmCraft compatibility settings.
- Three Settings pages: Launcher Settings, Appearance, and Updates.
- System, Light, Dark, True Dark, and optional Aetherglass appearance. Glass uses supported Windows 11 DWM behavior with a solid fallback.
- Background release-metadata checks once per startup, manual checks, explicit single-app installation, and sequential Update All.
- Native progress bars with download bytes, percentages when totals are reliable, and completed-app counts for batches.
- Retained older versions for manual rollback and Open Apps Folder for manual management.
- A console agent for JSON commands and on-demand MCP over stdio.

Checking metadata does not install updates. Successful verified installations become the preferred version unless a concurrent root or version-choice change must be preserved; the launcher reports these conflicts.

The launcher finds, launches, and updates Craft apps. [ArtCraft Skills](https://github.com/magejosh/artcraft-skills) provides skills that guide an assistant in using those apps.

## Documentation

- [Usage and portable settings](docs/USAGE.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md)
- [CLI and MCP integration](docs/AGENT.md)
- [v0.1.0 release notes and verification limits](RELEASE_NOTES.md)

## Building from source

The project uses Rust, the Windows API, and the MSVC x64 target. Dependencies are pinned in `Cargo.lock`. The supplied `build.ps1` is a maintainer-local script: it requires an E: checkout, a preinstalled local Rust toolchain/cache, and Visual Studio 18 Build Tools. Review and adapt that storage/toolchain policy for another machine. This restriction applies to the script, not the portable runtime.

In its configured environment, `./build.ps1 -Test` runs offline locked tests, and `./build.ps1 -StageOnly` stages both executables and their pair manifest under `.build/candidates/agent-control-mcp`. It does not replace `dist`.

The tested release process disabled release LTO through process-local build settings; `Cargo.toml` defaults to LTO enabled. A build using the Cargo defaults is not guaranteed to reproduce the published binaries byte for byte.

No project license grant is declared here. Dependencies and bundled notices retain their own terms.
