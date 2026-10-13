# Release notes

## v0.1.0

First release of the portable native Windows x64 Craft App Launcher.

- Choose an apps root without a fixed drive or folder requirement; preserve same-drive relative layouts when moving the launcher and apps together.
- Discover twelve Craft app families without executing them, report conservative PE versions, select installed versions, and retain older copies for rollback.
- Launch app GUIs from tiles or use the console agent for explicit GUI/CLI actions and on-demand stdio MCP.
- Use named profiles, literal per-app arguments, and opt-in computer-scoped FilmCraft DX12/exact-Obsidian-PATH compatibility.
- Choose System, Light, Dark, True Dark, or Aetherglass appearance through three Settings pages.
- Check metadata at startup or manually; explicitly install one verified portable release or run sequential Update All.
- Follow per-app download/install outcomes with native progress bars, bytes, reliable percentages, and a completed-app batch count.
- Prefer newly verified installations while preserving concurrent root/version choices. Keep older versions and historical stages available for manual management.

Ship the GUI, console agent, and pair manifest together. See [Usage](docs/USAGE.md) and [Agent integration](docs/AGENT.md).

### Verification and limits

Release qualification recorded 35 focused checks plus one additional completion check. Runtime checks covered seven read-only CLI/MCP interface checks and confirmed the added buttons and 13 native progress bars. Offline installer fixtures exercised validation, publication, progress, and cleanup behavior.

This does not establish universal live-download behavior: no real bulk download was performed during this qualification. Broad physical mouse/keyboard, accessibility, DPI, and visual coverage remain incomplete. The actual reparse-point fixture skipped because Windows denied the required privilege (error 1314). Native acrylic appearance still needs verification on supported Windows 11; Windows 10 uses the solid fallback. FilmCraft DXC behavior remains untested.

Successful cleanup targets only freshly owned, identity-checked artifacts and empty stage directories. Failed or changed stages and unknown files are preserved. Historical folders were not purged.
