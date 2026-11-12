import { signal, type Signal } from "nsh";
import { ToastManager } from "nsh/base";
import { MAX_TOASTS, type ToastMessage, type ToastTone } from "../components/ui";

const DURATION_MS: Record<ToastTone, number> = {
  success: 4500,
  info: 4500,
  warning: 8000,
  error: 8000,
};

/** How often the headless manager's lifecycle is advanced while any toast is mounted. */
const TICK_MS = 50;

interface ToastContent {
  tone: ToastTone;
  message: string;
}

export interface Toasts {
  readonly list: Signal<ToastMessage[]>;
  push(tone: ToastTone, message: string): void;
  dismiss(id: string): void;
  /** Escape or the back button while the stack holds toasts: dismiss the newest. */
  dismissNewest(): void;
  /** The pointer is on the stack: hold the timeouts while its toasts are read. */
  setExpanded(expanded: boolean): void;
}

/** The app's transient messages, kept by nsh/base's headless ToastManager. */
export function createToasts(): Toasts {
  const manager = new ToastManager();
  const list = signal<ToastMessage[]>([]);
  let sequence = 0;
  let paused = false;
  let timer: ReturnType<typeof setInterval> | undefined;

  /** Renders the newest toasts plus any that are leaving with their exit motion. */
  function sync(): void {
    list.set(
      manager.visible(MAX_TOASTS).map((entry) => {
        const { tone, message } = entry.value as ToastContent;
        return { id: entry.id, tone, message, status: entry.status };
      }),
    );
  }

  function tick(): void {
    if (manager.advance(paused).changed) {
      sync();
    }
    if (manager.isEmpty() && timer !== undefined) {
      clearInterval(timer);
      timer = undefined;
    }
  }

  function startTicking(): void {
    if (timer === undefined) {
      timer = setInterval(tick, TICK_MS);
    }
  }

  function push(tone: ToastTone, message: string): void {
    const id = `toast-${++sequence}`;
    manager.push(id, { tone, message }, { timeout: DURATION_MS[tone] });

    // Keep the on-screen count bounded: the oldest leave as a new one arrives.
    const active = manager.entries().filter((entry) => entry.status !== "ending");
    for (const old of active.slice(0, Math.max(0, active.length - MAX_TOASTS))) {
      manager.dismiss(old.id);
    }

    sync();
    startTicking();
  }

  function dismiss(id: string): void {
    if (!manager.dismiss(id)) {
      return;
    }
    sync();
    startTicking();
  }

  function dismissNewest(): void {
    const active = manager.entries().filter((entry) => entry.status !== "ending");
    const newest = active[active.length - 1];
    if (newest !== undefined) {
      dismiss(newest.id);
    }
  }

  return {
    list,
    push,
    dismiss,
    dismissNewest,
    setExpanded: (expanded) => {
      paused = expanded;
    },
  };
}
