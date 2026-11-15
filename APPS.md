# Contract of nas-sync-manager with the NAS launcher

The contract every application shares with the launcher (package, manifest, permissions, frame
protocol, launcher socket and providers) is `docs/development/external_apps/APPS.md` in the
nas-launcher repository, and the bridge is the one of `@valeriogc/nas-base-app`. This file keeps
only what is specific to nas-sync-manager, which builds and releases on its own.

## Identity

- id `nas-sync-manager`, category `storage`
- permissions `paths` ["/srv/storage"], `network` ["sync"], `packages` ["syncthing"]

## Specific to this application

`nas-sync-manager` requests `/srv/storage`, `permissions.network: ["sync"]` and the Debian package
`syncthing`. Its engine uses a private Unix API inside its state directory and its sandbox.
`PUT /api/apps/self/sync` with `{ active: boolean }` opens or closes the fixed local-network sync
ports through the launcher. `PUT /api/apps/self/background` with `{ needed: boolean }` keeps the
application running while at least one sync is active. The initial database contains no syncs.

The client API is reached by the desktop client through the gateway only, which checks the
certificate of the computer and sets `x-nas-client`; revocation from the Security center drops
the computer here too. See [DEVELOPMENT.md](DEVELOPMENT.md).

The commands that check the repository are in [DEVELOPMENT.md](DEVELOPMENT.md).
