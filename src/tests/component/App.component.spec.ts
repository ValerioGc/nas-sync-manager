import { mount, flushPromises } from '@vue/test-utils';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import App from '@/App.vue';
import { syncApi } from '@/services/sync';
import { listed, device, detail } from '@tests/fixtures/sync';
import { setLocale } from '@/i18n';
beforeEach(() => {
  setLocale('it');
  vi.spyOn(syncApi, 'status').mockResolvedValue({
    installed: true,
    running: true,
    failing: false,
    deviceId: 'nas-id',
  });
  vi.spyOn(syncApi, 'syncs').mockResolvedValue([listed]);
  vi.spyOn(syncApi, 'devices').mockResolvedValue([device]);
  vi.spyOn(syncApi, 'volumes').mockResolvedValue([{ id: 'disk', name: 'Dati' }]);
  vi.spyOn(syncApi, 'detail').mockResolvedValue({
    ...detail,
    conflicts: [{ path: 'copy.txt', original: 'a.txt', size: 100 }],
  });
  vi.spyOn(syncApi, 'pause').mockResolvedValue();
  vi.spyOn(syncApi, 'revoke').mockResolvedValue();
  vi.spyOn(syncApi, 'revokeDevice').mockResolvedValue();
  vi.spyOn(syncApi, 'resolve').mockResolvedValue();
  vi.spyOn(syncApi, 'settings').mockResolvedValue({
    maxSendKbps: 0,
    maxRecvKbps: 0,
    rescanMinutes: 60,
  });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});
it('loads the list, pauses, opens details and resolves a conflict with confirmation', async () => {
  const wrapper = mount(App);
  await flushPromises();
  expect(wrapper.text()).toContain('/Dati/Foto');
  await wrapper.get('[data-testid="pause_nas-photo"]').trigger('click');
  await flushPromises();
  expect(syncApi.pause).toHaveBeenCalledWith(listed.id, true);
  await wrapper.get('[data-testid="detail_nas-photo"]').trigger('click');
  await flushPromises();
  await wrapper.get('[data-testid="detail_pause"]').trigger('click');
  await flushPromises();
  for (const keep of ['this', 'other'] as const) {
    await wrapper.get(`[data-testid="keep_${keep}_copy.txt"]`).trigger('click');
    expect(wrapper.get('dialog').text()).toContain('cestino');
    await wrapper.get('[data-testid="confirm_action"]').trigger('click');
    await flushPromises();
    expect(syncApi.resolve).toHaveBeenCalledWith(listed.id, 'copy.txt', keep);
  }
  await wrapper.get('[data-testid="detail_revoke"]').trigger('click');
  await wrapper.get('[data-testid="confirm_action"]').trigger('click');
  await flushPromises();
  expect(syncApi.revoke).toHaveBeenCalledWith(listed.id);
  await wrapper.get('[data-testid="detail_close"]').trigger('click');
  expect(wrapper.find('[data-testid="sync_detail"]').exists()).toBe(false);
  wrapper.unmount();
});
it('cancels sync revocation and revokes a device only after confirmation', async () => {
  const wrapper = mount(App);
  await flushPromises();
  await wrapper.get('[data-testid="revoke_nas-photo"]').trigger('click');
  await wrapper.get('[data-testid="confirm_cancel"]').trigger('click');
  expect(syncApi.revoke).not.toHaveBeenCalled();
  await wrapper.get('[data-testid="nav_devices"]').trigger('click');
  await wrapper.get('[data-testid="device_revoke_pc"]').trigger('click');
  expect(wrapper.get('dialog').text()).toContain('computer');
  await wrapper.get('[data-testid="confirm_action"]').trigger('click');
  await flushPromises();
  expect(syncApi.revokeDevice).toHaveBeenCalledWith('pc');
  await wrapper.get('[data-testid="nav_syncs"]').trigger('click');
  await wrapper.get('[data-testid="nav_settings"]').trigger('click');
  await flushPromises();
  await wrapper.get('[data-testid="settings_close"]').trigger('click');
  expect(wrapper.find('[data-testid="settings_send"]').exists()).toBe(false);
  wrapper.unmount();
});
it('shows engine status and request failures with retry', async () => {
  vi.mocked(syncApi.status).mockRejectedValueOnce(Error('unreachable'));
  const wrapper = mount(App);
  await flushPromises();
  expect(wrapper.text()).toContain('unreachable');
  const engine = () => wrapper.get('[data-testid="engine_status"]');
  const status = (installed: boolean, failing: boolean) =>
    vi.mocked(syncApi.status).mockResolvedValue({
      installed,
      running: false,
      failing,
      deviceId: null,
    });
  status(false, false);
  await wrapper.get('[data-testid="retry"]').trigger('click');
  await flushPromises();
  expect(engine().text()).toContain('Motore di sincronizzazione non installato');
  expect(engine().attributes('role')).toBe('alert');
  status(true, true);
  await wrapper.get('[data-testid="refresh"]').trigger('click');
  await flushPromises();
  expect(engine().text()).toContain('Motore di sincronizzazione non avviato');
  expect(engine().attributes('role')).toBe('alert');
  status(true, false);
  await wrapper.get('[data-testid="refresh"]').trigger('click');
  await flushPromises();
  expect(engine().text()).toContain('Avvio del motore di sincronizzazione');
  expect(engine().attributes('role')).toBe('status');
  vi.mocked(syncApi.revoke).mockRejectedValueOnce(Error('refused'));
  await wrapper.get('[data-testid="revoke_nas-photo"]').trigger('click');
  await wrapper.get('[data-testid="confirm_action"]').trigger('click');
  await flushPromises();
  expect(wrapper.get('dialog').text()).toContain('refused');
  await wrapper.get('[data-testid="confirm_cancel"]').trigger('click');
  wrapper.unmount();
});
it('polls while idle and releases the timer on unmount', async () => {
  vi.useFakeTimers();
  const wrapper = mount(App);
  await flushPromises();
  await vi.advanceTimersByTimeAsync(5000);
  expect(syncApi.syncs).toHaveBeenCalledTimes(2);
  await wrapper.get('[data-testid="nav_settings"]').trigger('click');
  await flushPromises();
  await vi.advanceTimersByTimeAsync(5000);
  expect(syncApi.syncs).toHaveBeenCalledTimes(2);
  wrapper.unmount();
  await vi.advanceTimersByTimeAsync(5000);
  expect(syncApi.syncs).toHaveBeenCalledTimes(2);
});
