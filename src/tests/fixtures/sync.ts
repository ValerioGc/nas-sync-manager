import type { ListedSync, SyncDetail, Device } from '@/types/sync';
export const listed: ListedSync = {
  id: 'nas-photo',
  device: 'pc',
  name: 'Foto',
  nasPath: 'disk/Foto',
  pcPath: '/home/Foto',
  mode: 'two-way',
  paused: false,
  revoked: false,
  createdAt: 1,
  deviceName: 'Portatile',
  state: 'syncing',
  progress: 50,
  globalBytes: 2048,
  globalFiles: 2,
  needBytes: 1024,
  needFiles: 1,
  lastSync: null,
  error: null,
};
export const device: Device = {
  id: 'pc',
  name: 'Portatile',
  clientVersion: '0.1.0',
  pairedAt: 1,
  online: true,
  lastSeen: null,
  syncs: 1,
};
export const detail: SyncDetail = { sync: listed, changes: [], ignores: [], conflicts: [] };
