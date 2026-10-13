-- The settings of the sync: one row, none until they are changed from their defaults.
CREATE TABLE settings (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    max_send_kbps  INTEGER NOT NULL,
    max_recv_kbps  INTEGER NOT NULL,
    rescan_minutes INTEGER NOT NULL
) STRICT;

-- The computers paired with the NAS, by the identity of their Syncthing.
CREATE TABLE devices (
    id             TEXT    PRIMARY KEY,
    name           TEXT    NOT NULL,
    client_version TEXT    NOT NULL,
    paired_at      INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- The syncs, each between a folder of a computer and a folder of the data disks. A revoked one
-- stays until its computer removes it, so the client can say it was revoked.
CREATE TABLE syncs (
    id         TEXT    PRIMARY KEY,
    device     TEXT    NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    nas_path   TEXT    NOT NULL,
    pc_path    TEXT    NOT NULL,
    mode       TEXT    NOT NULL CHECK (mode IN ('two-way', 'pc-to-nas', 'nas-to-pc')),
    paused     INTEGER NOT NULL CHECK (paused IN (0, 1)),
    revoked    INTEGER NOT NULL CHECK (revoked IN (0, 1)),
    created_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
