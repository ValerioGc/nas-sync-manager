# NAS package of nas-sync-manager

Identity `nas-sync-manager`, initial version `0.1.0`. `pnpm run nas:package` produces the Linux
tar.gz and its checksum in `release-assets/`. It contains `nas-app.json`, `icon.svg`, `dist/` and
`bin/nas-sync-manager-axum`. `pnpm run nas:test` extracts and starts the package in an isolated
directory, and checks UI, assets, `/health`, version and the absence of TCP listeners of the app.

The launcher serves the app under `/apps/nas-sync-manager/`, talks to it over a Unix socket, grants
the data disks (`/srv/storage`, group `nas-storage`, `NAS_STORAGE_REGISTRY`) and installs the Debian
package `syncthing` the manifest asks for. `permissions.network: ["sync"]` grants the engine the
local network only; the app declares the active pairs with `PUT /api/apps/self/sync`. The inbound
ports stay closed while no pair is active. Background work is declared with
`PUT /api/apps/self/background`.

Database `sync-manager.sqlite3` and Syncthing's home in the systemd state folder. Syncthing uses a
Unix API socket and a private key, with its process supervised by the app. Neither the manifest nor
the package installs a global Syncthing service, and neither changes console, SSH or web ports. The
first installation has an empty database. The firewall and the real process inside the sandbox
still need the VM test described in [DEVELOPMENT.md](DEVELOPMENT.md).

CI, build and audit workflows as in the other NAS apps; metadata and version consistent across
Cargo, package, Sonar and changelog. The iframe protocol is in [APPS.md](APPS.md). The computers reach
the client routes through the gateway, with their certificate.
