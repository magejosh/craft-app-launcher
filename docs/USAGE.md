# Usage

## Apps folder and discovery

Choose a folder containing your portable Craft installations. Browse validates, saves, and scans the chosen folder; use Scan after typing a path. Failed validation preserves the previous saved root. Discovery runs in the background.

An installation is recognized by the exact, case-insensitive `<app-id>.exe` and/or `<app-id>-cli.exe` filename, such as `filmcraft.exe`. Identification never runs the executable. These names are not proof of authenticity: use apps from sources you trust.

The scanner searches up to four levels below the root and at most 20,000 directory entries. It stops descending into recognized installation folders. Keep versions in sibling folders. Junctions and other reparse points are excluded, including roots reached through reparse-point ancestors. Scan warnings explain inaccessible entries and limits.

Installed versions come from fixed PE file-version metadata, using the GUI executable when available. Missing evidence displays Unknown. A recognized folder naming pattern may supply a labeled folder hint; it is not proof of the executable's version. Automatic update decisions skip unsupported or uncertain version evidence.

## Launch and rollback

Click a tile for its selected GUI. Use its context menu or App settings to choose an installed version. Multiple installations need an explicit choice unless a saved choice remains valid. CLI launches are available through the agent with an explicit installation and profile.

App settings stores one literal argument per line. For example:

```text
--output
C:\Videos\My result.mp4
```

Do not add quotes merely because a path contains spaces. No shell parses these arguments. Blank lines are omitted; empty arguments and embedded newlines are not represented. A successful launch means Windows created the process; it does not establish that the app initialized successfully.

Older installations are retained. Select an older version to roll back. Open Apps Folder opens the saved root for manual file management. Check an app's own data compatibility before switching versions or removing folders.

## Profiles and portability

Named profiles keep independent per-app arguments. The apps root and installation choices are shared. Profile selection and FilmCraft compatibility are associated with the current Windows computer identity. If that identity cannot be read, manual profile selection lasts for the session and compatibility remains disabled.

`craft-launcher.conf` and `craft-launcher.log` live beside the launcher. Keep that directory writable. Roots on the same volume are stored relative to the executable directory, so moving the launcher and apps together with the same layout can preserve them across drive-letter changes. Cross-volume roots are absolute and may need repair after a move. Shortcut working directories do not determine the root.

Back up configuration before manual edits or launcher rollback. Invalid settings are reported instead of silently replaced. Older launchers may not support the current configuration format. Logs include paths, process IDs, and errors; arguments are omitted. Review logs before sharing them.

## FilmCraft compatibility

Compatibility is opt-in and scoped to the selected profile and computer. Choose the exact Obsidian installation directory before enabling it. FilmCraft child processes receive `WGPU_BACKEND=dx12`, and PATH entries matching only that normalized directory are removed from the child's environment. Similarly named directories remain. The parent, user, and system environment are not changed. Both GUI and CLI FilmCraft launches use the enabled profile.

This is not a DXC or playback compatibility guarantee. DXC behavior was not validated for this release.

## Appearance

Appearance offers System, Light, Dark, True Dark, and Aetherglass. Aetherglass depends on supported Windows 11 DWM features and falls back to a solid background when unavailable. Windows 10 fallback was exercised; native acrylic appearance on a supported Windows 11 desktop remains unverified.

## Updates

Startup checks official release metadata once in the background. Use Updates for manual checks. Downloads require an explicit installation or Update All action.

For a single installation, check the release, select an eligible portable asset, and confirm installation. The installer validates official metadata, size, digest, archive paths, expected executable, and version before publishing a new sibling folder.

Update All checks installed apps sequentially and uses the same verified installation path. Unknown versions, ambiguous assets, and unsupported releases are skipped. If any installed copy already has the latest or a newer version, the launcher avoids a redundant download even when an older copy is selected. Independent failures do not prevent other eligible apps from being attempted.

Rows progress through Downloading, Installing, and Installed, with Failed and Skipped outcomes shown separately. Downloading shows received bytes and a percentage only when a reliable total is available. Validation and extraction use an indeterminate bar. The batch display counts completed apps against its total.

A newly verified installation normally becomes preferred. Changes you make to the root or selected version while work is running are preserved, with a warning when they prevent automatic selection. Review the completion report.

Cancellation and root changes stop remaining work; an in-flight request can wait for its timeout. A published installation remains installed. Successful cleanup is limited to the identity-checked owned ZIP and fresh empty staging directories. Failures, unknown added files, changed directories, and historical stages are retained for inspection. Existing apps and older versions are never automatically purged.
