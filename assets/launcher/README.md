# Prism Orbit launcher icon

Original vector artwork for Craft App Launcher: a faceted luminous glass core,
one circular workflow rail, and three colored satellites. No third-party app
logos or fonts are used in the icon.

- `craft-app-launcher.svg`: editable 512-unit vector master.
- `craft-app-launcher.png`: transparent 512 px header/preview export.
- `craft-app-launcher.ico`: transparent 32-bit PNG-backed frames at 16, 24, 32,
  48, 64, 128, and 256 px.
- `launcher.rc`: Windows icon group 101. Keep its ID aligned with
  `launcher_icon()` in `src/native.rs`.

The MSVC build script compiles this resource using the installed Windows SDK
`rc.exe`, resolved through the compiler environment configured by `build.ps1`.
`RC` can specify an explicit compiler path. No additional Cargo dependency is
required. The resource is linked only into the GUI executable. Both launcher
and settings window classes load it at the current system large/small icon
dimensions with shared process-lifetime handles.

Design exports were generated with native System.Drawing from the SVG's circle,
polygon, and circular arc geometry. GDI radial interpolation approximates SVG
radial gradients. The reproducible renderer and dark/light size sheet are in
`.build/circle-proposal-20261016/icon-render.ps1` and
`.build/circle-proposal-20261016/icon-size-checksheet.png`.

No executable build, resource compilation, application launch, or runtime icon
verification was performed for this source-only proposal. Verify Explorer,
title bar, taskbar, Alt-Tab, and scaled-display appearance during the separately
authorized build/runtime phase.
