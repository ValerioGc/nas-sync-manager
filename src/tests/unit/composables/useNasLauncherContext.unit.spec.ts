import { afterEach, expect, it, vi } from 'vitest';
import { defineComponent, h, nextTick, type ComputedRef, type Ref } from 'vue';
import { mount } from '@vue/test-utils';
import { useNasLauncherContext } from '@/composables/useNasLauncherContext';
import { messages, setLocale } from '@/i18n';

let nasContextActive: Readonly<Ref<boolean>>;
let translations: ComputedRef<unknown>;
const Host = defineComponent({
  setup() {
    const { locale, t, active } = useNasLauncherContext();
    nasContextActive = active;
    translations = t;
    return () => h('span', locale.value);
  },
});
let originalParent: PropertyDescriptor | undefined;
afterEach(() => {
  if (originalParent) {
    Object.defineProperty(window, 'parent', originalParent);
  }
  vi.unstubAllGlobals();
});

it('accepts launcher context only from the same-origin parent', async () => {
  originalParent = Object.getOwnPropertyDescriptor(window, 'parent');
  const parent = { postMessage: vi.fn() };
  Object.defineProperty(window, 'parent', { configurable: true, value: parent });
  vi.stubGlobal('CSS', { supports: vi.fn().mockReturnValue(true) });
  const wrapper = mount(Host);
  expect(parent.postMessage).toHaveBeenCalledWith(
    { nas: 1, type: 'ready' },
    window.location.origin,
  );
  const context = { app: 'nas-backup-manager', theme: 'dark', accent: '#123456', locale: 'en' };
  for (const origin of ['https://other.example', window.location.origin]) {
    const event = new MessageEvent('message', {
      origin,
      data: { nas: 1, type: 'context', context },
    });
    Object.defineProperty(event, 'source', { value: parent });
    window.dispatchEvent(event);
    await nextTick();
    expect(wrapper.text()).toBe(origin === window.location.origin ? 'en' : 'it');
  }
  expect(document.documentElement.lang).toBe('en');
  expect(translations.value).toBe(messages.en);
  expect(document.documentElement.dataset.theme).toBe('dark');
  expect(document.documentElement.style.getPropertyValue('--accent')).toBe('#123456');
  wrapper.unmount();
});

it('starts from the url and keeps the defaults for an unsupported context', async () => {
  originalParent = Object.getOwnPropertyDescriptor(window, 'parent');
  const parent = { postMessage: vi.fn() };
  Object.defineProperty(window, 'parent', { configurable: true, value: parent });
  vi.stubGlobal('CSS', { supports: vi.fn().mockReturnValue(false) });
  const initialUrl = window.location.href;
  window.history.replaceState(null, '', '?lang=en&theme=dark');
  const wrapper = mount(Host);
  expect(wrapper.text()).toBe('en');
  expect(document.documentElement.dataset.theme).toBe('dark');
  expect(nasContextActive.value).toBe(false);

  function arrive(data: unknown) {
    const event = new MessageEvent('message', { origin: window.location.origin, data });
    Object.defineProperty(event, 'source', { value: parent });
    window.dispatchEvent(event);
  }
  arrive({ nas: 1, type: 'described', notifications: [] });
  expect(nasContextActive.value).toBe(false);
  document.documentElement.style.setProperty('--accent', '#123456');
  arrive({
    nas: 1,
    type: 'context',
    context: { app: 'nas-backup-manager', theme: 'light', accent: 'nope', locale: 'fr' },
  });
  await nextTick();
  expect(nasContextActive.value).toBe(true);
  expect(wrapper.text()).toBe('en');
  expect(document.documentElement.dataset.theme).toBe('light');
  expect(document.documentElement.style.getPropertyValue('--accent')).toBe('');
  wrapper.unmount();
  expect(nasContextActive.value).toBe(false);
  window.history.replaceState(null, '', initialUrl);
  setLocale('it');
});

it('ignores an unknown language and theme in the url', () => {
  const initialUrl = window.location.href;
  window.history.replaceState(null, '', '?lang=fr&theme=blue');
  const wrapper = mount(Host);
  expect(wrapper.text()).toBe('it');
  expect(document.documentElement.dataset.theme).toBe('light');
  wrapper.unmount();
  window.history.replaceState(null, '', initialUrl);
});
