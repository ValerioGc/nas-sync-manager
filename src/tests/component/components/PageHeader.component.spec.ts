import { mount } from '@vue/test-utils';
import { expect, it } from 'vitest';
import PageHeader from '@/components/PageHeader.vue';
it('titles the section and requests a refresh unless busy', async () => {
  const wrapper = mount(PageHeader, { props: { section: 'syncs', busy: false } });
  expect(wrapper.get('h1').text()).toBe('Sincronizzazioni');
  await wrapper.get('[data-testid="refresh"]').trigger('click');
  expect(wrapper.emitted('refresh')).toHaveLength(1);
  await wrapper.setProps({ section: 'devices', busy: true });
  expect(wrapper.get('h1').text()).toBe('Dispositivi');
  expect(wrapper.get('[data-testid="refresh"]').attributes('disabled')).toBeDefined();
  wrapper.unmount();
});
