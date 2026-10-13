# NAS application contract

This document describes the common NAS app contract, used here by nas-sync-manager. Each repository builds and releases independently. The temporary `external-apps/` directory in a NAS checkout is not a dependency.

## Package and runtime

`app.system` is an optional boolean, defaulting to `false`, independent of `app.category`.
External system apps retain their own process, package updates and management. The catalogue
can mark an entry with `system`; `preinstall` selects initial installation with a desktop
shortcut, independently of the preference for later interactive installs.

`nas-app.json` declares `manifest_version: 1`, the stable app ID and version, `run.exec`, `run.socket`, `ui.protocol: 1`, capabilities, and requested paths. The packager copies `src/assets/logo.svg` to `icon.svg` inside the archive. `pnpm run nas:package` builds the frontend and backend and writes the archive and checksum. `pnpm run nas:test` extracts the archive in an isolated directory and checks static assets, the JSON `/health` version, API response, executable permissions, and absence of a public TCP listener. The NAS launcher installs, starts, stops, updates, and uninstalls each archive. Its app proxy serves it under `/apps/<id>/` on the launcher's origin.

The backend listens on the Unix socket passed by `--listen` or `NAS_APP_SOCKET`. Development uses `pnpm run backend:dev` and Vite on `127.0.0.1:1420`; Vite proxies `/api/` to that socket. Frontend requests must use relative `api/...` URLs so that the same code works below `/apps/<id>/`. The frontend bundle uses `base: './'`; it must not import files from this document's host repository. `data-testid` attributes exist in development tests and are removed from production output.

nas-file-manager is a web application only. Its NAS backend reads `NAS_STORAGE_ROOT` (set on the NAS by the launcher's grant of `/srv/storage`; `FS_EXPLORER_ROOT` in development), uses `STATE_DIRECTORY` for preview cache when present, and provides `/api/launcher/search`. Its manifest requests `/srv/storage` and declares `search` and `file-picker` capabilities; no other app declares a file capability.

## Permissions

`nas-app@.service` hides from every application the data disks (`/srv`, `/mnt`, `/media`), the logs, the launcher's state except the application's own package, `/sys`, `/run/udev` and the processes of other users. `permissions.paths` asks for more, from a fixed list the launcher grants without asking: `/srv/storage` (read and written, in the `nas-storage` group, with `NAS_STORAGE_ROOT` and `NAS_STORAGE_REGISTRY` set), and read only `/proc` (all of it, not only the processes), `/sys` and `/run/udev`. A package asking for any other path is refused at installation and leaves nothing on disk. nas-file-manager and nas-backup-manager ask for `/srv/storage`, nas-disk-manager for `/proc`, `/sys` and `/run/udev`, the others for nothing.

`permissions.cleanup: true` (nas-cleanup-manager) lets an application ask the launcher, on its socket, for the cleanups of a fixed list (`POST /api/apps/self/cleanup` with `{ action: 'overview' | 'preview' | 'clean', item?, options: { temporaryDays, journalMb } }`) and read and empty the caches the applications declare (`GET /api/caches`, `DELETE /api/caches/{app}/{id}`). It sends only identifiers and two numbers: the paths and the commands are the launcher's, run as root by `nas-os-cleanup.service`.

## Launcher iframe protocol

All messages carry `nas: 1`. The launcher accepts messages only from the registered iframe window and its expected origin. Applications accept shell messages only from `window.parent` with `event.origin === window.location.origin`. Applications send `ready` after mounting their message listener, to `window.location.origin`; standalone pages send nothing to themselves. The shell replies with `context` containing `app`, resolved `theme` (`light` or `dark`), `accent`, `locale`, and `palette`: the colours of the launcher's theme by role (`background`, `foreground`, `surface`, `sunken`, `hover`, `border`, `muted`), which the application puts in its `--nas-<role>` tokens on the root and removes when missing or invalid. Context can arrive repeatedly. The shell controls theme, accent, and language in NAS iframe mode; standalone and native preferences remain available outside it. An empty accent removes the inline override.

Application-to-shell messages are `ready`, `notify`, `open`, `describe`, `pick-files`, `network-disks`, and `power-schedule`. The shell returns `context`, `described` notification labels, `pick-files-result`, `network-disks-result`, and `power-schedule-result`. The `open` request may carry `location`, a query string beginning with `?` and at most 2048 characters. The common TypeScript contract, the transport (`createNasShell`) and the Vue lifecycle (`useNasLauncherContext`) live in the shared library `@valeriogc/nas-base-app`. No application imports launcher source files.

An application process has no loopback (`IPAddressDeny=localhost` in the NAS unit) and never calls the launcher on `127.0.0.1:3000`. To leave a notification or report an operation from its backend it uses the unix socket named by `NAS_LAUNCHER_SOCKET`, where the launcher identifies the application from its systemd unit and accepts writes only under its own id. The routes are listed in the launcher's `docs/development/external_apps/APPS.md`.

nas-file-manager alone may request a folder shortcut:

```ts
{ nas: 1, type: 'shortcut', requestId: string, path: string, name: string }
{ nas: 1, type: 'shortcut-result', requestId: string, success: boolean, error?: string }
```

The launcher derives the sender from its registered frame and persists the shortcut before returning success. The folder path is logical relative to Explorer's configured root. The external page never writes the launcher's desktop API directly. The launcher stores shortcuts of kind `app` or `folder`, opens folders in `nas-file-manager` with `?path=...`, and keeps shortcuts when Explorer is uninstalled so they work again after reinstalling it.

## Search provider

Explorer keeps `/api/search?query=` for its own UI. The launcher uses the manifest endpoint `/api/launcher/search` with `q`, optional `type`, and optional `ext`. The response is an array of `{ id, name, path, type, extension, size, modified, target }`. Types are `folder`, `document`, `image`, `audio`, `video`, and `archive`. `path` is a logical path such as `/Documents/report.pdf`; `target` is an encoded query such as `?path=%2FDocuments&select=report.pdf`. Explorer maps the query to its filesystem view, while the launcher assigns the provider app ID. The launcher can start a stopped provider through the existing app lifecycle before retrying its query. Missing metadata uses an empty `modified` string and numeric size zero.

## File picker provider

An application calls `shell.pickFiles({ multiple })` on the bridge of `@valeriogc/nas-base-app`. The launcher queries `GET /api/capabilities/file-picker`, starts the first installed version 1 provider if necessary, and opens its declared UI endpoint (ending in `/`) in a dialog. Explorer declares `endpoint: "/"`. The UI receives a `nasPicker` token and optional `multiple=1` in its query; `shell.pickFolder(directory?)` sends `folder: true` instead, the UI receives `folder=1`, lists only folders and returns the folder being browsed as `{ name, path }`, with no `download` and so no `url`. It lists folders and files from its own backend, then sends `{ nas: 1, type: 'files-picked', token, files: [{ name, path, download }], cancelled? }` to the shell. `download` is a relative `api/...` URL under the provider's `/apps/<id>/` path. Explorer uses `api/downloads?path=...`.

The shell accepts the result only from the picker iframe on the same origin with the matching token. It returns `{ nas: 1, type: 'pick-files-result', requestId, files: [{ name, path, url }], cancelled, error? }` to the requesting app. `url` is the absolute authenticated NAS URL for the provider's download. The caller fetches it with its NAS session; `path` is a logical path, not an unrestricted host filesystem path. Cancelling yields an empty array, and a missing or unavailable provider yields an `error`. Only one picker can be open; multiple selection accepts at most 100 files.

## Network disks provider

nas-disk-manager declares the `network-disks` capability at `/api/network-disks`, which answers with the network shares mounted on the host (`cifs`, `smb3`, `nfs`, `nfs4`, `fuse.sshfs`) in the same shape as `/api/disks`. An application sends `{ nas: 1, type: 'network-disks', requestId }` through `requestNetworkDisks()`; the launcher starts the first installed version 1 provider if necessary, reads the endpoint and returns `{ nas: 1, type: 'network-disks-result', requestId, success, data?: [{ name, mount, source, readOnly }], error? }`. nas-backup-manager uses it to let cloud backups choose only among those shares, and opens nas-disk-manager on that section with `location: '?kind=network'` when there is none.

Any application may read the power schedule of the NAS with `{ nas: 1, type: 'power-schedule', requestId }` through `requestPowerSchedule()`: the launcher answers `{ nas: 1, type: 'power-schedule-result', requestId, success, data?: { enabled, rules: [{ day, time, action }] }, error? }`, with only the active rules. nas-backup-manager uses it in the schedule step of a rule, to show when the NAS is off and to refuse a time that falls there.

## Repository gates

The four repositories expose the same pnpm script names. Locally they install with `pnpm install`; the pipelines install with `npm ci` from `package-lock.json`, which `pnpm install` keeps in step (`postinstall`), `pnpm run lock:npm` realigns after `pnpm add`, `update` or `remove`, and `pnpm run lint` checks. Run `pnpm run format:check`, `pnpm run lint`, `pnpm run type-check`, `pnpm test`, `pnpm run test:e2e`, `pnpm run test:coverage`, `cargo fmt --manifest-path backend/Cargo.toml --all --check`, `cargo clippy --manifest-path backend/Cargo.toml --all-targets -- -D warnings`, and `cargo test --locked --manifest-path backend/Cargo.toml`. `pnpm run nas:package` and `pnpm run nas:test` verify the standalone archive. `release:merge` writes Git history and tags; never use it for a test in a real app repository. Its no-tag case uses the version in `package.json`, and `--dry-run` makes no changes.

Frontend tests live under `src/tests/unit`, `src/tests/component`, `src/tests/scripts`, and `src/tests/e2e`, mirroring source paths. Rust tests live under `backend/tests/unit` and `backend/tests/integration` when applicable. UI strings belong in `src/i18n/locales/{en,it}.ts`. Source code, method names, technical errors, and test descriptions are in English; `README.md`, `DEVELOPMENT.md`, and `NAS.md` are in Italian.

## Synchronization app

`nas-sync-manager` requests `/srv/storage`, `permissions.network: ["sync"]` and the Debian package
`syncthing`. Its engine uses a private Unix API inside its state directory and its sandbox.
`PUT /api/apps/self/sync` with `{ active: boolean }` opens or closes the fixed local-network sync
ports through the launcher. `PUT /api/apps/self/background` with `{ needed: boolean }` keeps the
application running while at least one sync is active. The initial database contains no syncs.

The client API and its `x-nas-client` identity are internal. External certificate authentication
and security-center revocation are deferred to the client/gateway phase; the app cannot verify
a certificate from this header. See [DEVELOPMENT.md](DEVELOPMENT.md).
