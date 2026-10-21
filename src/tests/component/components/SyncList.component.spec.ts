import { mount } from '@vue/test-utils';
import { expect, it } from 'vitest';
import SyncList from '@/components/SyncList.vue';
import { listed } from '@tests/fixtures/sync';
it('names disks, shows progress and emits the selected actions', async () => {
  const wrapper = mount(SyncList, {
    props: { syncs: [listed], volumes: [{ id: 'disk', name: 'Dati' }], busy: false },
  });
  expect(wrapper.text()).toContain('/Dati/Foto');
  expect(wrapper.find('progress').attributes('value')).toBe('50');
  for (const action of ['detail', 'pause', 'revoke']) {
    await wrapper.get(`[data-testid="${action}_${listed.id}"]`).trigger('click');
  }
  expect(wrapper.emitted('detail')?.[0]).toEqual([listed.id]);
  expect(wrapper.emitted('pause')?.[0]).toEqual([listed]);
  expect(wrapper.emitted('revoke')?.[0]).toEqual([listed.id]);
  await wrapper.setProps({
    syncs: [{ ...listed, paused: true, state: 'paused', progress: null, error: 'disk full' }],
  });
  expect(wrapper.text()).toContain('Riprendi');
  expect(wrapper.text()).toContain('disk full');
  expect(wrapper.find('progress').exists()).toBe(false);
  await wrapper.setProps({ syncs: [{ ...listed, revoked: true }] });
  expect(wrapper.get(`[data-testid="pause_${listed.id}"]`).attributes('disabled')).toBeDefined();
  await wrapper.setProps({ syncs: [] });
  expect(wrapper.text()).toContain('Nessuna sincronizzazione');
  wrapper.unmount();
});
