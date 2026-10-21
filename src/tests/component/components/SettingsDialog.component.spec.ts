import { mount, flushPromises } from '@vue/test-utils';
import { afterEach, expect, it, vi } from 'vitest';
import SettingsDialog from '@/components/SettingsDialog.vue';
import { syncApi } from '@/services/sync';
afterEach(() => vi.restoreAllMocks());
async function type(wrapper: ReturnType<typeof mount>, testid: string, value: string) {
  const field = wrapper.get(`[data-testid="${testid}"]`);
  await field.setValue(value);
  await field.trigger('change');
}
it('loads, keeps the numbers in their bounds, steps them and saves', async () => {
  vi.spyOn(syncApi, 'settings').mockResolvedValue({
    maxSendKbps: 0,
    maxRecvKbps: 0,
    rescanMinutes: 60,
  });
  const save = vi
    .spyOn(syncApi, 'saveSettings')
    .mockResolvedValue({ maxSendKbps: 100, maxRecvKbps: 1, rescanMinutes: 60 });
  const wrapper = mount(SettingsDialog);
  await flushPromises();
  await type(wrapper, 'settings_send', '100');
  await type(wrapper, 'settings_rescan', '0');
  expect((wrapper.get('[data-testid="settings_rescan"]').element as HTMLInputElement).value).toBe(
    '60',
  );
  await wrapper.get('[data-testid="settings_receive_increase"]').trigger('click');
  await wrapper.get('form').trigger('submit');
  await flushPromises();
  expect(save).toHaveBeenCalledWith({ maxSendKbps: 100, maxRecvKbps: 1, rescanMinutes: 60 });
  expect(wrapper.emitted('saved')).toBeTruthy();
  expect(wrapper.emitted('close')).toBeTruthy();
  wrapper.unmount();
});
it('shows failures, retries loading, and preserves edits after save fails', async () => {
  vi.spyOn(syncApi, 'settings')
    .mockRejectedValueOnce(Error('gone'))
    .mockResolvedValue({ maxSendKbps: 0, maxRecvKbps: 0, rescanMinutes: 60 });
  vi.spyOn(syncApi, 'saveSettings').mockRejectedValue(Error('cannot save'));
  const wrapper = mount(SettingsDialog);
  await flushPromises();
  expect(wrapper.text()).toContain('gone');
  await wrapper.get('[role="alert"] button').trigger('click');
  await flushPromises();
  await wrapper.get('form').trigger('submit');
  await flushPromises();
  expect(wrapper.text()).toContain('cannot save');
  await wrapper.get('[data-testid="settings_close"]').trigger('click');
  expect(wrapper.emitted('close')).toBeTruthy();
  wrapper.unmount();
});
it('can close while loading or after a failed load', async () => {
  vi.spyOn(syncApi, 'settings').mockRejectedValue(Error('gone'));
  const wrapper = mount(SettingsDialog);
  expect(wrapper.text()).toContain('Caricamento');
  await flushPromises();
  await wrapper.get('[data-testid="settings_close"]').trigger('click');
  await wrapper.get('dialog').trigger('cancel');
  expect(wrapper.emitted('close')).toHaveLength(2);
  wrapper.unmount();
});
