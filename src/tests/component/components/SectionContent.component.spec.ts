import { mount } from '@vue/test-utils';
import { expect, it } from 'vitest';
import SectionContent from '@/components/SectionContent.vue';
import { detail, device, listed } from '@tests/fixtures/sync';
const props = {
  section: 'syncs' as const,
  loading: true,
  detail: null,
  syncs: [listed],
  devices: [device],
  volumes: [],
  busy: false,
  deviceId: 'nas-id',
};
it('switches between loading, lists and details and forwards the requests', async () => {
  const wrapper = mount(SectionContent, { props });
  expect(wrapper.text()).toContain('Caricamento in corso');
  expect(wrapper.get('.device_identity').text()).toContain('nas-id');
  await wrapper.setProps({ loading: false });
  await wrapper.get('[data-testid="detail_nas-photo"]').trigger('click');
  await wrapper.get('[data-testid="pause_nas-photo"]').trigger('click');
  await wrapper.get('[data-testid="revoke_nas-photo"]').trigger('click');
  expect(wrapper.emitted('detail')?.[0]).toEqual(['nas-photo']);
  expect(wrapper.emitted('pause')?.[0]).toEqual([listed]);
  expect(wrapper.emitted('request')?.[0]).toEqual([{ kind: 'sync', id: 'nas-photo' }]);
  await wrapper.setProps({
    detail: { ...detail, conflicts: [{ path: 'copy.txt', original: 'a.txt', size: 100 }] },
  });
  await wrapper.get('[data-testid="detail_pause"]').trigger('click');
  await wrapper.get('[data-testid="detail_revoke"]').trigger('click');
  await wrapper.get('[data-testid="keep_this_copy.txt"]').trigger('click');
  await wrapper.get('[data-testid="detail_close"]').trigger('click');
  expect(wrapper.emitted('pause')?.[1]).toEqual([listed]);
  expect(wrapper.emitted('request')?.slice(1)).toEqual([
    [{ kind: 'sync', id: 'nas-photo' }],
    [{ kind: 'conflict', id: 'nas-photo', path: 'copy.txt', keep: 'this' }],
  ]);
  expect(wrapper.emitted('close')).toHaveLength(1);
  await wrapper.setProps({ detail: null, section: 'devices', deviceId: null });
  await wrapper.get('[data-testid="device_revoke_pc"]').trigger('click');
  expect(wrapper.emitted('request')?.[3]).toEqual([{ kind: 'device', id: 'pc' }]);
  expect(wrapper.text()).not.toContain('ID dispositivo del NAS');
  wrapper.unmount();
});
