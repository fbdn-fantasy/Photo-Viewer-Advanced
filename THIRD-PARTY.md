# Dependency and licensing information

Photo Viewer Advanced 3.0.0 ships under the Apache License, Version 2.0. See `LICENSE` at the source root or `resources/licenses/LICENSE` in the installation. The prior application copyright and MIT notice are preserved in `resources/licenses/Original-MIT.txt` in the installation and under `app/src-tauri/` at the same relative path in source. The supplied PVA icon remains user-provided artwork under its existing rights.

The installer contains complete notices in `resources/licenses/`:

- `RUST-DEPENDENCIES.txt`: resolved Windows x64 Rust dependencies, versions, license identifiers and full license texts.
- `NPM-DEPENDENCIES.txt`: frontend/runtime and build packages with versions and full notices.
- `ImageMagick-LICENSE.txt` and `ImageMagick-and-delegates-NOTICE.txt`: ImageMagick 7.1.2-35 Q16-HDRI and its linked codec libraries, including exact source commits.
- `MICROSOFT-VC-RUNTIME.txt`: redistribution terms for the unmodified bundled x64 OpenMP runtime, `vcomp140.dll`.
- `DEPENDENCIES.txt`: installed component overview.

ImageMagick is developed by ImageMagick Studio LLC. Photo Viewer Advanced uses its unmodified executable as a separate process with a restrictive local policy. This does not imply ImageMagick endorsement. Important image libraries include LibRaw, libheif/libde265, libjxl, OpenJPEG, OpenEXR, libwebp, libpng, libjpeg-turbo, libtiff, librsvg and Little CMS. Each retains its own license.

To rebuild or modify the bundled decoder, use `Photo-Viewer-Advanced-3.0.0-Source.zip`, the companion archive for the Windows installer. Inside that ZIP, `third-party-sources/README.md` gives the rebuild instructions. The same directory contains 49 pinned upstream archives, their SHA-256 manifest, Windows build configuration, and sources for the linked LGPL libraries and MPL Rust crates.

Tauri and WebView2 are separate components. The Rust app and frontend assets are bundled in the executable. Microsoft Edge WebView2 is a system runtime; Microsoft's unmodified offline runtime installer is embedded in setup, and Microsoft's runtime terms apply. No source archive or neighboring workspace files are needed to run the installed app.

