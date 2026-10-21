import type { Device, ListedSync, Settings, Status, SyncDetail, Volume } from '@/types/sync';

async function request<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
  const response = await fetch(`api/${path}`, {
    method,
    ...(body === undefined
      ? {}
      : { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }),
  });
  if (!response.ok) {
    const answer: unknown = await response.json().catch(() => null);
    const error = answer && typeof answer === 'object' && 'error' in answer ? answer.error : null;
    throw new Error(typeof error === 'string' ? error : `HTTP ${response.status}`);
  }
  return response.status === 204 ? (undefined as T) : (response.json() as Promise<T>);
}

const idPath = (id: string) => `syncs/${encodeURIComponent(id)}`;

export const syncApi = {
  status: () => request<Status>('status'),
  syncs: () => request<ListedSync[]>('syncs'),
  devices: () => request<Device[]>('devices'),
  volumes: () => request<Volume[]>('volumes'),
  detail: (id: string) => request<SyncDetail>(idPath(id)),
  pause: (id: string, paused: boolean) =>
    request<void>(`${idPath(id)}/${paused ? 'pause' : 'resume'}`, 'POST'),
  revoke: (id: string) => request<void>(`${idPath(id)}/revoke`, 'POST'),
  revokeDevice: (id: string) => request<void>(`devices/${encodeURIComponent(id)}`, 'DELETE'),
  resolve: (id: string, path: string, keep: 'this' | 'other') =>
    request<void>(`${idPath(id)}/conflicts`, 'POST', { path, keep }),
  settings: () => request<Settings>('settings'),
  saveSettings: (settings: Settings) => request<Settings>('settings', 'PUT', settings),
};

export function diskPath(path: string, volumes: Volume[]): string {
  const parts = path.replace(/^\/+/, '').split('/');
  const volume = volumes.find((volume) => volume.id === parts[0]);
  if (volume) {
    parts[0] = volume.name;
  }

  return `/${parts.join('/')}`;
}

export function formatDate(value: string | number | null, locale: string): string {
  if (value === null) {
    return '-';
  }

  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '-' : date.toLocaleString(locale);
}
