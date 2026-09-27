import { createI18n } from "vue-i18n";
import en from "./locales/en.json";
import ru from "./locales/ru.json";

export const messages = { ru, en };
export type Language = keyof typeof messages;

export const i18n = createI18n({ legacy: false, locale: "en", fallbackLocale: "en", messages });

export function systemLanguage(): Language {
  return navigator.language.toLowerCase().startsWith("ru") ? "ru" : "en";
}

export function setLanguage(lang: string) {
  if (lang in messages) i18n.global.locale.value = lang as Language;
}
