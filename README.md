# nas-sync-manager

NAS application "Synchronization": it shows and manages the folder pairs between computers and the
NAS, with Syncthing as a child process inside the sandbox of the app. It works on the local
network, without global discovery, relays, NAT traversal, telemetry or automatic updates of the
engine.

- **Synchronizations**: name, NAS folder with the name of the disk, computer and computer folder,
  mode, state, progress and last synchronization. Pause, resume and revoke.
- **Details**: files, bytes, last changes, exclusions present on the NAS and conflict copies. The
  current file or the copy can be kept; the discarded one goes to the Explorer trash when on,
  otherwise it is deleted permanently, after confirmation.
- **Devices**: paired computers, identity, client version, state, last contact and revoke.
- **Settings**: bandwidth limits, on the LAN too (0 means no limit), and a full scan every 1-1440
  minutes, hourly by default, besides the real-time watching.

Pairs are created and edited from the API of the client. The revocation of a pair stays visible to
the client until it removes it; that of a computer removes all its pairs. The files on the NAS
stay. Sync folders cannot overlap; a revoked folder can be paired again. The trash and the NAS
metadata are excluded from the sync, even when a whole disk is chosen.

The app declares background work and sync ports to the launcher while at least one pair is active.
At the first installation there are no pairs and the ports stay closed. Syncthing talks to the app
through a private Unix socket and a persistent key; if it stops it is started again and receives
its configuration from the database again. The page shows the state of the engine in a banner:
starting, not started (the last start failed, a new attempt follows by itself) or not installed.

Computers pair through the desktop client NAS Sync (`destkop-apps/nas-sync-client`): the gateway
gives each computer a certificate once the account password is confirmed, and lets only that
certificate reach the client routes, naming the computer in `x-nas-client`. Revoking the computer
in the Security center drops its pairs here too. The exclusions of the PC do not move
automatically with Syncthing: the `.stignore` files of the NAS are shown here; the client does
not send its own exclusions yet.

## Local development

Requires Node.js 24, pnpm and Rust/Cargo. Installing Syncthing is needed only for a real test.

```bash
pnpm install
pnpm run build
pnpm run backend:dev
```

In another terminal: `pnpm run dev`. The UI is on `http://127.0.0.1:1420/`, with a proxy to the
socket of the backend. `NAS_STORAGE_ROOT` can point to a test folder, `NAS_SYNCTHING` to the binary
of the engine; without Syncthing the UI shows the problem and creates no pairs. Use only test
folders: a real sync can change or delete files.

For structure and checks see [DEVELOPMENT.md](DEVELOPMENT.md), for the NAS contract
[APPS.md](APPS.md), for the package [NAS.md](NAS.md). The tests use a simulated Syncthing: real
transport of files, Debian installation, firewall and restart of the NAS are to be tried on a VM.
