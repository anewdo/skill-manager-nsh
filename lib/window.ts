/** The starting window size, in points. Mirrors `nsh.json`'s `window.defaultSize`. */
export const INITIAL_WINDOW_WIDTH = 1200;
export const INITIAL_WINDOW_HEIGHT = 760;

/** The gap kept between the window and the edges of a small display. */
const DISPLAY_MARGIN = 48;

/** An area in points, in screen coordinates (`nsh/desktop`'s `Bounds`). */
export interface WorkArea {
  origin: { x: number; y: number };
  size: { width: number; height: number };
}

/** A display's identity and the area windows may use on it. */
export interface WindowDisplay {
  primary: boolean;
  visibleBounds: WorkArea;
}

/** The work area of the primary display, or the first one when none is primary. */
export function primaryWorkArea(displays: readonly WindowDisplay[]): WorkArea | null {
  return (displays.find((display) => display.primary) ?? displays[0])?.visibleBounds ?? null;
}

/** The starting window bounds: centered in the work area, shrunk to fit small displays. */
export function startingWindowBounds(area: WorkArea | null | undefined): {
  origin: { x: number; y: number };
  size: { width: number; height: number };
} | null {
  if (!area) {
    return null;
  }
  const width = Math.max(1, Math.min(INITIAL_WINDOW_WIDTH, area.size.width - DISPLAY_MARGIN));
  const height = Math.max(1, Math.min(INITIAL_WINDOW_HEIGHT, area.size.height - DISPLAY_MARGIN));
  return {
    size: { width, height },
    origin: {
      x: Math.round(area.origin.x + (area.size.width - width) / 2),
      y: Math.round(area.origin.y + (area.size.height - height) / 2),
    },
  };
}
