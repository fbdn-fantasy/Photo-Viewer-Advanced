# Bundled decoder and copyleft dependency sources

These are unmodified upstream source archives corresponding to the decoder bundled with Photo Viewer Advanced 1.0.0. They are supplied with the application source release; the installed application does not need or download them.

`manifest.json` records each upstream repository, exact commit or crate version, download URL, byte size and SHA-256. The 38 decoder component commits come directly from the binary distribution's `NOTICE.txt`. The ImageMagick commit is `5d1bcc55f2c83bc2ff802118b7369756610f3ad4` (7.1.2-35). The Configure and Dependencies tags are pinned in that commit's release build scripts. The Windows orchestration scripts are the latest commit preceding the release. The additional 12-bit and 16-bit JPEG sources and dependency Configure version are pinned by the Dependencies scripts.

The five `.crate` archives are the unmodified MPL-2.0 Rust dependencies used in the Windows dependency graph. A crate is a gzip-compressed tar archive. Each archive includes its source, and the full MPL license is also supplied in `src-tauri/resources/licenses/RUST-DEPENDENCIES.txt`.

All third-party code remains under its own license. Full decoder/delegate notices are in `src-tauri/resources/licenses/ImageMagick-and-delegates-NOTICE.txt`. The project license is in the root LICENSE file.

## Verify or restore archives

Run `node third-party-sources/fetch-sources.mjs` from the source root to verify hashes and retrieve a missing archive from its pinned upstream URL. Existing matching archives are reused.

Run `powershell -ExecutionPolicy Bypass -File third-party-sources/restore-decoder-sources.ps1` to extract a separate `decoder-rebuild` source tree. It refuses to overwrite an existing tree. No compiler or network access is needed for this extraction.

## Rebuild or replace the decoder

The extracted tree follows the upstream Windows and Dependencies repository layout. The upstream instructions are in `decoder-rebuild/Readme.md` and `decoder-rebuild/Dependencies/Readme.md`; release build commands are in the respective `.github` directories. Build requirements include Visual Studio C++ desktop development, ATL/MFC and the Windows SDK. The distributed upstream binary was built using Visual Studio 2026; rebuilding it has not been performed as part of the Photo Viewer Advanced validation.

Build the dependency Configure solution under `Dependencies/Configure`, configure/build the static x64 dependency libraries with OpenMP and the linked C runtime, then build the main Configure solution under `Configure`. Use the main configuration equivalent to `/noWizard /VS2026 /hdri /Q16 /x64 /static /linkRuntime`, matching the pinned ImageMagick `.github/workflows/release.yml`. The upstream scripts document generated solution and artifact locations. Build the complete executable with modified LGPL components if desired; all component sources and configuration sources needed for that source rebuild are provided here.

Photo Viewer Advanced invokes `resources/imagemagick/magick.exe` as a separate process. You may replace this executable with a compatible rebuilt version without modifying the app. Keep the accompanying configuration and color profile files, and supply any DLL dependencies required by your replacement. Do not change the safe input restrictions in `policy.xml` unless you understand the consequences. There is no app signature check, DRM, or restriction on replacing/debugging the decoder for personal modification.

The bundled `vcomp140.dll` is the unmodified Microsoft x64 OpenMP runtime from Visual Studio 2022 14.44.35211. Its license is supplied in `MICROSOFT-VC-RUNTIME.txt`; it is not part of the open-source decoder archives. Other DLL imports of the bundled decoder are Windows system libraries.
