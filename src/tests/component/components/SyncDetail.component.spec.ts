import { mount } from '@vue/test-utils';
import { expect, it } from 'vitest';
import SyncDetail from '@/components/SyncDetail.vue';
import { detail } from '@tests/fixtures/sync';
it('shows empty details then file changes, exclusions and conflict choices', async () => {
  const wrapper = mount(SyncDetail, { props: { detail, volumes: [], busy: false } });
  expect(wrapper.text()).toContain('Nessun conflitto');
  expect(wrapper.text()).toContain('Nessun percorso escluso');
  expect(wrapper.text()).toContain('Nessun cambiamento');
  expect(wrapper.find('[data-testid="detail_trash_off"]').exists()).toBe(false);
  await wrapper.setProps({ detail: { ...detail, sync: { ...detail.sync, trashOff: true } } });
  expect(wrapper.get('[data-testid="detail_trash_off"]').text()).toContain(
    'cancellati definitivamente',
  );
  await wrapper.setProps({
    detail: {
      ...detail,
      sync: { ...detail.sync, paused: true, progress: null, error: 'full' },
      conflicts: [{ path: 'copy.txt', original: 'a.txt', size: 100 }],
      ignores: ['*.tmp'],
      changes: [
        { time: '2026-10-11T12:00:00Z', action: 'modified', path: 'a.txt' },
        { time: '2026-10-11T12:00:00Z', action: 'unknown', path: 'b.txt' },
      ],
    },
  });
  expect(wrapper.text()).toContain('*.tmp');
  expect(wrapper.text()).toContain('Modificato a.txt');
  expect(wrapper.text()).toContain('unknown b.txt');
  expect(wrapper.text()).toContain('full');
  await wrapper.get('[data-testid="keep_this_copy.txt"]').trigger('click');
  await wrapper.get('[data-testid="keep_other_copy.txt"]').trigger('click');
  expect(wrapper.emitted('resolve')).toEqual([
    ['copy.txt', 'this'],
    ['copy.txt', 'other'],
  ]);
  for (const action of ['close', 'pause', 'revoke']) {
    await wrapper.get(`[data-testid="detail_${action}"]`).trigger('click');
  }
  expect(wrapper.emitted('close')).toBeTruthy();
  expect(wrapper.emitted('pause')).toBeTruthy();
  expect(wrapper.emitted('revoke')).toBeTruthy();
  wrapper.unmount();
});
