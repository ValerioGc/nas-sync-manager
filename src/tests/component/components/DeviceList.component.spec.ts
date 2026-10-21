import { mount } from '@vue/test-utils';
import { expect, it } from 'vitest';
import DeviceList from '@/components/DeviceList.vue';
import { device } from '@tests/fixtures/sync';
it('shows computer connectivity and requests revocation', async () => {
  const wrapper = mount(DeviceList, { props: { devices: [device], busy: false } });
  expect(wrapper.text()).toContain('In linea');
  await wrapper.get('[data-testid="device_revoke_pc"]').trigger('click');
  expect(wrapper.emitted('revoke')?.[0]).toEqual(['pc']);
  await wrapper.setProps({ devices: [{ ...device, online: false }], busy: true });
  expect(wrapper.text()).toContain('Non in linea');
  expect(wrapper.get('button').attributes('disabled')).toBeDefined();
  await wrapper.setProps({ devices: [] });
  expect(wrapper.text()).toContain('Nessun computer');
  wrapper.unmount();
});
