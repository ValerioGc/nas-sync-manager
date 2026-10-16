# Development of nas-sync-manager

Vue/TypeScript in `src/`, Rust/Axum in `backend/axum/`. The launcher bridge, shared styles, scripts
and pipelines come from the library `@valeriogc/nas-base-app` (command `nas-base-app`). The app
uses the dependencies of the other NAS apps; it includes no Tauri. Version and identity are in
`package.json`, `nas-app.json`, Cargo, Sonar and `CHANGELOG.txt`.

## Structure

- `App.vue`: layout of the library (`NasAppLayout`), loading, refresh every five seconds, the state
  of the engine and confirmed actions.
- `components/`: lists of the syncs and of the devices, details and settings; the confirmations use
  `NasConfirmDialog` of the library, the settings `NasModal` and `NasNumberStepper`.
- `services/sync.ts`, `types/sync.ts`: relative API `api/...`, types and names of the disks.
- `service.rs`: pairs, devices, state, folder choices and conflicts; it serializes the mutations
  to avoid overlapping pairs created at the same time. Disk I/O on the blocking pool. A failed
  reconciliation is retried; the declarations to the launcher are renewed every 30 seconds to
  recover from temporary errors of the ports or of the background.
- `syncthing.rs`: child process (`STMONITORED=1`: one process, no monitor of its own, so a stop
  leaves no child behind), restart, API on a private socket and key passed through
  `STGUIAPIKEY`, absent from the arguments of the process, `HOME` set to its own folder (the
  dynamic user of the service has none); LAN-only settings already in the first `config.xml`,
  bandwidth limits, folders and the trash excluded from the sync. `failing` in `GET /api/status`
  says the last start failed.
- `store.rs`, `0001_sync.sql`: SQLite, settings, devices and pairs.
- `storage.rs`, `trash.rs`: disks granted by the launcher and external versioning for the trash of
  Explorer. Paths cannot leave the folder or the trash through symbolic links.
- `main.rs`: UI, API, start and the command `--trash <folder> <file>` called by Syncthing.

## HTTP contract

API of the page: `GET /api/status`, `/api/volumes`, `/api/syncs`, `/api/syncs/{id}`,
`POST /api/syncs/{id}/pause`, `/resume`, `/revoke`, `/conflicts` (body `{ path, keep }`, `keep` is
`this` or `other`); `GET /api/devices`, `DELETE /api/devices/{id}`; `GET` and `PUT /api/settings`
(`maxSendKbps`, `maxRecvKbps`, `rescanMinutes`).

Client API, reached by NAS Sync through the gateway: `POST /api/client/pair` (`deviceId`, `name`,
`version`, where `deviceId` must be the computer named in `x-nas-client`),
`GET /api/client/folders?path=...`, `GET` and `POST /api/client/syncs`, `PUT` and
`DELETE /api/client/syncs/{id}`. Creation receives `name`, `nasPath`, `pcPath`, `mode`; edits can
change `name`, `pcPath`, `mode`, `paused`. The modes are `two-way`, `pc-to-nas`, `nas-to-pc`. A
client sees and edits only its own pairs.

`x-nas-client` is set only by the gateway: it drops the header from every external request and
writes it on `/client/sync/...` after checking the certificate of the computer, which it issues
only with the account password. A certificate pairs once. Revoking or removing the computer in
the Security center calls `DELETE /api/devices/{id}`, retried by the gateway until it succeeds.

## Conventions and dependencies

Composition API, `<script setup lang="ts">`, `@/` imports, scoped styles with SCSS tokens, texts in
`src/i18n/locales/it.ts` and `en.ts`, shared types in `src/types/`. Code, tests and documentation
in English. No new dependency without authorization.

Locally pnpm; in CI `npm ci`. `package-lock.json` changes only with `pnpm run lock:npm` or the pnpm
postinstall; `Cargo.lock` only with Cargo. `pnpm run lint` checks the npm lock.

## Shared library

The launcher bridge, styles, shared components and helpers, and the build, package and release scripts come from `@valeriogc/nas-base-app`, published on GitHub Packages; the commands of `package.json` use its `nas-base-app`. The `.npmrc` of the project points the `@valeriogc` scope to its registry; locally a classic token with `read:packages` is needed in your own `~/.npmrc` (`//npm.pkg.github.com/:_authToken=...`), while the pipelines use the token of the workflow. For a new version of the library, update the number in `package.json` and run `pnpm install`. The local `pnpm-workspace.yaml` has `'@valeriogc/nas-base-app': false` under `allowBuilds`.

## Checks

```bash
pnpm run format:check
pnpm run lint
pnpm run type-check
pnpm test
pnpm run test:e2e
pnpm run test:coverage
cargo fmt --manifest-path backend/Cargo.toml --all --check
cargo clippy --manifest-path backend/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path backend/Cargo.toml
pnpm run coverage:rust
pnpm run nas:package
pnpm run nas:test
```

The FE tests are in `src/tests/`, the Rust ones in `backend/tests/unit/axum/`. The Rust and E2E
tests use a simulated engine that answers on the Unix REST API, without syncing files between
machines. The E2E tests start the real backend, a fake launcher and the fake engine, with isolated
database and folders; they check the UI and the real requests. They need no Syncthing installed.
The FE coverage must exceed 80% for lines, functions, branches and statements; check the Rust one
too. Do not mistake these results for a test of the real Syncthing transport.

## Test on a Debian VM, still to run

Install the updated packages of wizard and launcher and the app. Check: ports closed without pairs,
granted disks, process in the sandbox, Unix-only API, pairing of a second Syncthing engine on the
LAN, the three modes, conflicts, files with spaces, trash on and off, bandwidth limits, restart of
the child, reboot and an unplugged disk. For now the client API is to be called only as a
development tool on the VM, until the gateway supports the certificates of the PCs. Pausing or
revoking the last pair must close the ports. Stop from console/SSH:
`sudo systemctl stop nas-app@nas-sync-manager`; no recovery procedure was tried here.
