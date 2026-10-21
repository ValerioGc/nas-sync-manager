import { computed } from 'vue';
import { useNasLauncherContext as useLauncherContext } from '@valeriogc/nas-base-app';
import { locale, messages, setLocale } from '@/i18n';

/** Launcher context with the app messages in the active locale. */
export function useNasLauncherContext() {
  const { active } = useLauncherContext({ onLocaleChange: setLocale });
  const t = computed(() => messages[locale.value]);

  return { locale, t, active };
}
