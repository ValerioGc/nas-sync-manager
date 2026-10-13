import { computed } from 'vue';
import { useNasLauncherContext as useLauncherContext } from '@valeriogc/nas-base-app';
import { locale, messages, setLocale } from '@/i18n';

/** The context of the launcher, with the catalogue of the application in its language. */
export function useNasLauncherContext() {
  const { active } = useLauncherContext({ onLocaleChange: setLocale });
  const t = computed(() => messages[locale.value]);
  return { locale, t, active };
}
