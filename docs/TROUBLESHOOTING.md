# Troubleshooting

| Symptom | What to check |
| --- | --- |
| Windows reports missing VCRUNTIME140.dll | Install the official Microsoft Visual C++ v14 x64 Redistributable linked in the README. |
| No apps found | Select the actual apps root. Keep exact app executable names and place separate versions in sibling folders. Review depth, entry-limit, and access warnings. |
| Saved root is missing after moving folders | Browse to the new root. Same-drive relative layouts can move together; cross-drive roots are absolute. |
| A junction or symlink is rejected | Choose a normal directory reached through normal ancestors. The launcher excludes reparse points. |
| Version shows Unknown or a folder hint | The executable lacks sufficient PE version evidence. A folder name is not treated as a verified version. |
| Several installations exist but launch is unavailable | Choose the version explicitly in the context menu or App settings. |
| Settings cannot save | Keep the launcher in a writable folder. Preserve the existing configuration and read the reported validation or conflict error. |
| Settings changed externally | Reload and review the current values before retrying. Do not overwrite another change blindly. |
| Agent reports pair mismatch or unavailable | Restore all three matched release files together. Do not mix the GUI, agent, or manifest from different builds. |
| Agent reports owner_busy | Another operation owns the launcher. Wait and reconcile state before issuing another mutation. |
| Update is skipped | Read the per-app result. Unknown versions, an existing latest/newer installation, or missing/ambiguous verified assets can cause a skip. |
| Update fails or is canceled | Read the completion report and log. Failed stages can remain for inspection. Confirm the actual installed state before retrying. |
| A new version installed but was not selected | A concurrent preference/root change can preserve your choice. Review the warning, then select the version manually if desired. |
| Progress has no percentage | The total is not reliable, or verification/extraction is running. Indeterminate progress is expected. |
| Old folders remain | Older versions and historical stages are intentionally retained. Use Open Apps Folder and manage them manually after inspection. |
| Glass appears solid | The system may not support the requested DWM effect. The solid fallback is expected on Windows 10. |
| FilmCraft compatibility is unavailable | Check the current profile, readable computer identity, and exact existing Obsidian directory. This option does not validate DXC or app playback. |

For a bug report, include launcher version, Windows version, steps, expected/actual result, and the relevant error. Review logs and screenshots for private paths or other personal information before sharing. Do not include your whole configuration by default.
