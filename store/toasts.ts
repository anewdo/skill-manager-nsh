import { signal, type Signal } from "nsh";
import type { ToastMessage, ToastTone } from "../components/ui";

const DURATION_MS: Record<ToastTone, number> = {
  success: 4500,
  info: 4500,
  warning: 8000,
  error: 8000,
};
const MAX_TOASTS = 3;

export interface Toasts {
  readonly list: Signal<ToastMessage[]>;
  push(tone: ToastTone, message: string): void;
  dismiss(id: number): void;
}

/** The app's transient messages: at most three, each leaving by itself. */
export function createToasts(): Toasts {
  const list = signal<ToastMessage[]>([]);
  const timers = new Map<number, ReturnType<typeof setTimeout>>();
  let sequence = 0;

  function stopTimer(id: number): void {
    const timer = timers.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      timers.delete(id);
    }
  }

  function dismiss(id: number): void {
    stopTimer(id);
    list.set(list().filter((toast) => toast.id !== id));
  }

  function push(tone: ToastTone, message: string): void {
    const id = ++sequence;
    const next = [...list(), { id, tone, message }];
    while (next.length > MAX_TOASTS) {
      stopTimer(next[0].id);
      next.shift();
    }
    list.set(next);
    timers.set(
      id,
      setTimeout(() => dismiss(id), DURATION_MS[tone]),
    );
  }

  return { list, push, dismiss };
}
