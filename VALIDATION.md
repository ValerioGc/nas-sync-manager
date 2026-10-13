# Validation of the NAS sync, 2026-10-11

## State found

The Rust backend was implemented: SQLite, supervised Syncthing, internal API of the clients, lists,
pauses, revocations, conflicts, trash and declarations to the launcher. `App.vue`, the components
of the interface and the FE/E2E tests of the sync were missing. The tests of the routes held only
an import; those of the disks did not compile because of the changed signature of `Scratch::new`.
README, DEVELOPMENT, NAS, changelog, language catalogues and logo were still those of Cleanup. The
trash/network phases already had code and tests of their own in the other projects.

## Done

- NAS interface with syncs, devices, details, progress, conflicts, exclusions, settings and
  confirmations; names of the disks, Italian/English and theme received from the launcher.
- Fixed the incomplete tests; HTTP tests on the real routes and on the errors, tests of the restart
  of the engine, of the resumption of the configuration and of concurrent mutations.
- LAN-only settings from the first start, private API key and directory, LAN bandwidth limit.
- Exclusion of the trash/NAS metadata; reserved folders refused and vanished folders not created
  again.
- Validation of symbolic links in the paths of the trash and of the conflicts; disk operations off
  the async thread; overlapping pairs prevented even with concurrent requests.
- Engine errors propagated in pauses and revocations: no success answer if Syncthing refuses the
  removal. Revoked folders can be paired again.
- Progress also read from the receiving PC: an idle NAS does not mean the PC is already in sync.
- Documentation and logo of the sync, removal of unused inherited icons, NAS package.

## Local results

`npm run release:test` completed: release metadata, formatting, lint, npm lock, types, tests, E2E,
`cargo fmt`, Clippy with warnings forbidden, build and check of the package.

| Check                                                                            | Result                            |
| -------------------------------------------------------------------------------- | --------------------------------- |
| FE unit and convention tests                                                     | 17 passed                         |
| FE component tests                                                               | 11 passed                         |
| Script/package/release tests                                                     | 29 passed                         |
| Playwright Chromium, real backend and simulated Syncthing                        | 2 passed                          |
| Rust tests of the sync                                                           | 39 passed                         |
| FE coverage lines / statements / functions / branches                            | 98.03% / 98.14% / 98.98% / 95.91% |
| Rust coverage lines / regions / functions                                        | 93.61% / 89.64% / 88.08%          |
| NAS package and checksum                                                         | Checked                           |
| Start from the package, UI and assets, health/version, Unix socket only, SIGTERM | Checked                           |

## Review of the NAS plan

The second review also checked the code of the integrations against the contract of the engine,
and fixed:

- The answer `ignore: null` of Syncthing when `.stignore` is missing, refused by the code before
  and not reproduced by the simulator. Now the Rust simulator represents that case too. The folder
  is configured paused and started only after the mandatory exclusions; an error reading or
  writing the exclusions leaves it paused.
- Incomplete reconciliations considered successful before, so never retried. Now failed folders
  and settings are retried even after the engine was already configured; the updates of the
  settings are serialized with the other mutations. The NAS cannot pair with itself.
- Port declarations not applied again after a failure of the helper, if the request file had
  already been written. The launcher serializes and applies the declarations again, and the sync
  renews ports and background every 30 seconds.
- TCP 21027 opened needlessly: TCP is limited to 22000, UDP to 22000/21027, on IPv4 and IPv6 LAN.
- Paths of the Explorer trash that could follow symbolic directories. The operations refuse those
  paths, the root is not treated as an entry to restore or delete, and the expiry accepts only the
  trashes of the granted disks. Links as single entries are not followed.
- Permissions of the new subfolders of the trash: explicit `2770` to share them in the storage
  group between Explorer, sync and SMB.
- Expiry dates tied to the path instead of the version, and the ctime of the parents that made
  the siblings younger. The Explorer migration `0006_trash_versions.sql` adds the signature of the
  version, keeps the existing dates and records every descendant of a deleted folder. An external
  replacement does not inherit the old expiry; deleted empty directories expire too, kept for the
  same period.
- Syncthing API key exposed in the arguments of the process: it goes through `STGUIAPIKEY`.
- Incomplete async mocks in the component suites of Explorer and Cleanup, which caused unhandled
  errors while showing the single assertions green.

| Project  | Checks of the review                                                                                          |
| -------- | ------------------------------------------------------------------------------------------------------------- |
| Sync     | Full release, 39 Rust, 17 unit/convention + 11 component, 29 Node tests, 2 E2E                                |
| Explorer | Full release and final Rust/package check, 88 Rust, 138 unit/convention + 69 component, 29 Node tests, 14 E2E |
| Launcher | Format, lint, build, 477 unit/script + 406 component, 319 Rust, 22 E2E                                        |
| Wizard   | Format, lint, build, 78 unit/script + 59 component, 57 Rust, 4 E2E                                            |
| Cleanup  | Full release, 31 Rust, 19 unit/convention + 23 component, 29 Node tests, 4 E2E                                |
| Backup   | Whole Rust suite, 75 passed, including trash exclusions and watcher                                           |

Clippy and `cargo fmt` completed for Sync, Explorer, launcher, wizard and Cleanup. The full
releases also include package, checksum and its start. No release was published. Explorer
coverage: FE lines 98.23%, branches 94.59%, functions 97.65%, statements 98.15%; Rust lines 88.71%,
regions 86.19%, functions 89.62%. Cleanup FE coverage: lines 97.31%, branches 94.52%, functions
92.74%, statements 96.43%. Coverage of the sync in the table above. Launcher FE coverage: lines
94.38%, branches 90.57%, functions 92.21%, statements 94.24%; wizard: lines 95.71%, branches 91.85%,
functions 96.15%, statements 95.73%. Rust branch coverage is not turned on; the percentages are
project totals.

A pre-existing test of the power scheduler of the launcher failed once under parallel load: its
30 ms limit had already passed at the first query, skipping the waiting notification. The whole
suite passed limited to four threads, and it had passed in the first check too. This outcome is a
fragility of the test, not evidence of a regression of the sync. The E2E tests of the apps use the
same port 1420: they were run again in sequence after a timeout caused by reusing the server of
another suite.

The system helpers were tried with fake commands and temporary folders; no rule was applied to the
machine. The tests of the sync package do not start a Syncthing possibly installed on the host.
The image of the list was inspected; the table scrolls horizontally in narrow windows.

Local environment: Node.js 26.8.1 available on the host, stable Rust and dependencies already
present, without adding dependencies. Project and pipelines stay configured for Node.js 24.
Coverage in `coverage/` and `backend/coverage/lcov.info`; package and SHA256 in `release-assets/`.

## Left to the next phase or to the VM

Tauri was not implemented. Pairing the PCs with NAS credentials, certificates of the gateway and
integration with the Security center are postponed to the phase of the Tauri client, as the plan
foresees. `x-nas-client` is an internal identity: not an authentication ready for external access.
The exclusions shown are those of Syncthing on the NAS; the future client will have to send those
of the computer, because `.stignore` is not synchronized automatically.

The tests simulate the Syncthing API: they do not prove the real transfer of files between
machines, the three modes with a Debian engine, the real bandwidth limits, the systemd sandbox, the
installed firewall, a reboot or a physically unplugged disk. The VM test is described in
[DEVELOPMENT.md](DEVELOPMENT.md). No service was installed or enabled on the host. Without active
pairs the app declares the ports closed; no real sync was created here.

References used to check the contract of the engine:
[CLI and Unix socket](https://docs.syncthing.net/users/syncthing.html),
[options and LAN limits](https://docs.syncthing.net/users/config.html),
[external versioning](https://docs.syncthing.net/users/versioning.html),
[completion of the receiver](https://docs.syncthing.net/rest/db-completion-get.html).

The read and write contract of the exclusions was also compared with the
[official code of Syncthing 1.29.5](https://github.com/syncthing/syncthing/blob/v1.29.5/lib/model/model.go)
and the Samba configuration with the
[manual of the recycle module](https://www.samba.org/samba/docs/current/man-html/vfs_recycle.8.html).
This static comparison does not replace the test of the Debian package on a VM.
