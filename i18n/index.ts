import type { Language } from "../lib/types";
import { en } from "./en";
import { zhCN, type MessageKey } from "./zh_CN";

export type { MessageKey } from "./zh_CN";

export type MessageParams = Record<string, string | number>;
export type Translate = (key: MessageKey, params?: MessageParams) => string;

const tables: Record<Language, Record<MessageKey, string>> = { zh_cn: zhCN, en };

export function translate(language: Language): Translate {
  const table = tables[language];
  return (key, params) => {
    const template = table[key];
    if (!params) {
      return template;
    }
    return template.replace(/\{(\w+)\}/g, (match, name: string) => {
      const value = params[name];
      return value === undefined ? match : String(value);
    });
  };
}

export function messageKeys(): MessageKey[] {
  return Object.keys(zhCN) as MessageKey[];
}
