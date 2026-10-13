import { afterEach, describe, expect, it, vi } from 'vitest';
import { diskPath, formatDate, syncApi } from '@/services/sync';
afterEach(() => vi.unstubAllGlobals());
describe('sync API', () => {
  it('uses app-relative URLs and encodes identities for every route', async () => {
    const fetcher = vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => ({}) });
    vi.stubGlobal('fetch', fetcher);
    await syncApi.status();
    await syncApi.syncs();
    await syncApi.devices();
    await syncApi.volumes();
    await syncApi.detail('a/b');
    await syncApi.pause('a/b', true);
    await syncApi.pause('a/b', false);
    await syncApi.revoke('a/b');
    await syncApi.revokeDevice('a/b');
    await syncApi.resolve('a/b', 'Foto/a.txt', 'other');
    await syncApi.settings();
    await syncApi.saveSettings({ maxSendKbps: 10, maxRecvKbps: 20, rescanMinutes: 60 });
    expect(fetcher.mock.calls.map((call) => call[0])).toEqual([
      'api/status',
      'api/syncs',
      'api/devices',
      'api/volumes',
      'api/syncs/a%2Fb',
      'api/syncs/a%2Fb/pause',
      'api/syncs/a%2Fb/resume',
      'api/syncs/a%2Fb/revoke',
      'api/devices/a%2Fb',
      'api/syncs/a%2Fb/conflicts',
      'api/settings',
      'api/settings',
    ]);
    expect(fetcher.mock.calls[9]?.[1]).toMatchObject({
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: 'Foto/a.txt', keep: 'other' }),
    });
  });
  it('handles empty successful responses and readable backend errors', async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValueOnce({ ok: true, status: 204 })
      .mockResolvedValueOnce({ ok: false, status: 403, json: async () => ({ error: 'Revoked' }) })
      .mockResolvedValueOnce({
        ok: false,
        status: 500,
        json: async () => {
          throw Error('not JSON');
        },
      })
      .mockResolvedValueOnce({ ok: false, status: 400, json: async () => ({ error: 1 }) });
    vi.stubGlobal('fetch', fetcher);
    await expect(syncApi.revoke('a')).resolves.toBeUndefined();
    await expect(syncApi.syncs()).rejects.toThrow('Revoked');
    await expect(syncApi.syncs()).rejects.toThrow('HTTP 500');
    await expect(syncApi.syncs()).rejects.toThrow('HTTP 400');
  });
  it('displays disk names and handles missing dates', () => {
    expect(diskPath('/disk/Foto', [{ id: 'disk', name: 'Disco dati' }])).toBe('/Disco dati/Foto');
    expect(diskPath('missing/Foto', [])).toBe('/missing/Foto');
    expect(formatDate(null, 'it')).toBe('-');
    expect(formatDate('bad', 'it')).toBe('-');
    expect(formatDate(0, 'en')).toContain('1970');
  });
});
