export type SyncMode = 'two-way' | 'pc-to-nas' | 'nas-to-pc';
export type SyncState =
  'synced' | 'syncing' | 'scanning' | 'paused' | 'offline' | 'error' | 'unknown' | 'revoked';
export interface Sync {
  id: string;
  device: string;
  name: string;
  nasPath: string;
  pcPath: string;
  mode: SyncMode;
  paused: boolean;
  revoked: boolean;
  createdAt: number;
}
export interface ListedSync extends Sync {
  deviceName: string;
  state: SyncState;
  progress: number | null;
  globalBytes: number;
  globalFiles: number;
  needBytes: number;
  needFiles: number;
  lastSync: string | null;
  /** The trash of its disk is off: what the sync deletes on the NAS is gone for good. */
  trashOff: boolean;
  error: string | null;
}
export interface Device {
  id: string;
  name: string;
  clientVersion: string;
  pairedAt: number;
  online: boolean;
  lastSeen: string | null;
  syncs: number;
}
export interface Settings {
  maxSendKbps: number;
  maxRecvKbps: number;
  rescanMinutes: number;
}
export interface Status {
  installed: boolean;
  running: boolean;
  /** The last start failed: a new one follows by itself. */
  failing: boolean;
  deviceId: string | null;
}
export interface Volume {
  id: string;
  name: string;
}
export interface Conflict {
  path: string;
  original: string;
  size: number;
}
export interface Change {
  time: string;
  action: string;
  path: string;
  remote: boolean;
}
export interface SyncDetail {
  sync: ListedSync;
  changes: Change[];
  ignores: string[];
  conflicts: Conflict[];
}
export type Section = 'syncs' | 'devices';
