import { createNasI18n } from '@valeriogc/nas-base-app';
import { en } from '@/i18n/locales/en';
import { it } from '@/i18n/locales/it';

export const messages = { en, it } as const;
export type Locale = keyof typeof messages;
export const { locale, setLocale, useI18n } = createNasI18n(messages);
