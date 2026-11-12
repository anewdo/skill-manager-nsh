import { Image, token } from "nsh";

/**
 * The app's icon set. nsh ships no icons, so every icon is embedded here as
 * the body of an SVG: path data from lucide-static v1.70.0 (ISC License).
 * `Icon` draws one through `Image` and recolors it with the image's
 * `tintColor`, so icons follow the light and dark themes from one source.
 */

/** Marks the icon's opaque pixels; `tintColor` recolors all of them. */
const INK = "#000000";

const svg = (body: string): string => body;

const iconSources = {
  store: svg(
    `<path d="M15 21v-5a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v5"/>` +
      `<path d="M17.774 10.31a1.12 1.12 0 0 0-1.549 0 2.5 2.5 0 0 1-3.451 0 1.12 1.12 0 0 0-1.548 0 2.5 2.5 0 0 1-3.452 0 1.12 1.12 0 0 0-1.549 0 2.5 2.5 0 0 1-3.77-3.248l2.889-4.184A2 2 0 0 1 7 2h10a2 2 0 0 1 1.653.873l2.895 4.192a2.5 2.5 0 0 1-3.774 3.244"/>` +
      `<path d="M4 10.95V19a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8.05"/>`,
  ),
  library: svg(
    `<path d="m16 6 4 14"/>` +
      `<path d="M12 6v14"/>` +
      `<path d="M8 8v12"/>` +
      `<path d="M4 4v16"/>`,
  ),
  settings: svg(
    `<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/>` +
      `<circle cx="12" cy="12" r="3"/>`,
  ),
  search: svg(
    `<path d="m21 21-4.34-4.34"/>` + `<circle cx="11" cy="11" r="8"/>`,
  ),
  palette: svg(
    `<path d="M12 22a1 1 0 0 1 0-20 10 9 0 0 1 10 9 5 5 0 0 1-5 5h-2.25a1.75 1.75 0 0 0-1.4 2.8l.3.4a1.75 1.75 0 0 1-1.4 2.8z"/>` +
      `<circle cx="13.5" cy="6.5" r=".5" fill="#000000"/>` +
      `<circle cx="17.5" cy="10.5" r=".5" fill="#000000"/>` +
      `<circle cx="6.5" cy="12.5" r=".5" fill="#000000"/>` +
      `<circle cx="8.5" cy="7.5" r=".5" fill="#000000"/>`,
  ),
  globe: svg(
    `<circle cx="12" cy="12" r="10"/>` +
      `<path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/>` +
      `<path d="M2 12h20"/>`,
  ),
  folder: svg(
    `<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>`,
  ),
  refresh: svg(
    `<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/>` +
      `<path d="M21 3v5h-5"/>` +
      `<path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/>` +
      `<path d="M8 16H3v5"/>`,
  ),
  update: svg(
    `<circle cx="12" cy="12" r="10"/>` +
      `<path d="m16 12-4-4-4 4"/>` +
      `<path d="M12 16V8"/>`,
  ),
  download: svg(
    `<path d="M12 15V3"/>` +
      `<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>` +
      `<path d="m7 10 5 5 5-5"/>`,
  ),
  preview: svg(
    `<path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0"/>` +
      `<circle cx="12" cy="12" r="3"/>`,
  ),
  trash: svg(
    `<path d="M10 11v6"/>` +
      `<path d="M14 11v6"/>` +
      `<path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/>` +
      `<path d="M3 6h18"/>` +
      `<path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>`,
  ),
  rename: svg(
    `<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/>` +
      `<path d="m15 5 4 4"/>`,
  ),
  close: svg(`<path d="M18 6 6 18"/>` + `<path d="m6 6 12 12"/>`),
  chevronLeft: svg(`<path d="m15 18-6-6 6-6"/>`),
  chevronRight: svg(`<path d="m9 18 6-6-6-6"/>`),
  arrowUp: svg(`<path d="m5 12 7-7 7 7"/>` + `<path d="M12 19V5"/>`),
  list: svg(
    `<path d="M3 5h.01"/>` +
      `<path d="M3 12h.01"/>` +
      `<path d="M3 19h.01"/>` +
      `<path d="M8 5h13"/>` +
      `<path d="M8 12h13"/>` +
      `<path d="M8 19h13"/>`,
  ),
  grid: svg(
    `<rect width="7" height="7" x="3" y="3" rx="1"/>` +
      `<rect width="7" height="7" x="14" y="3" rx="1"/>` +
      `<rect width="7" height="7" x="14" y="14" rx="1"/>` +
      `<rect width="7" height="7" x="3" y="14" rx="1"/>`,
  ),
  check: svg(`<path d="M20 6 9 17l-5-5"/>`),
  warning: svg(
    `<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/>` +
      `<path d="M12 9v4"/>` +
      `<path d="M12 17h.01"/>`,
  ),
  error: svg(
    `<circle cx="12" cy="12" r="10"/>` +
      `<line x1="12" x2="12" y1="8" y2="12"/>` +
      `<line x1="12" x2="12.01" y1="16" y2="16"/>`,
  ),
  info: svg(
    `<circle cx="12" cy="12" r="10"/>` +
      `<path d="M12 16v-4"/>` +
      `<path d="M12 8h.01"/>`,
  ),
  minus: svg(`<path d="M5 12h14"/>`),
  circleCheck: svg(`<circle cx="12" cy="12" r="10"/>` + `<path d="m16 9-5.5 5.5L8 12"/>`),
  package: svg(
    `<path d="M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z"/>` +
      `<path d="M12 22V12"/>` +
      `<path d="m7.5 4.27 9 5.15"/>`,
  ),
  searchX: svg(
    `<path d="m13.5 8.5-5 5"/>` +
      `<path d="m8.5 8.5 5 5"/>` +
      `<circle cx="11" cy="11" r="8"/>` +
      `<path d="m21 21-4.3-4.3"/>`,
  ),
};

export type IconName = keyof typeof iconSources;

/** The theme tokens an icon may take its color from. */
export type IconColor =
  | "foreground"
  | "muted-foreground"
  | "primary"
  | "primary-foreground"
  | "secondary-foreground"
  | "accent-foreground"
  | "card-foreground"
  | "popover-foreground"
  | "destructive"
  | "success"
  | "warning"
  | "info";

export interface IconProps {
  name: IconName;
  /** Its drawn size in points, default 16. */
  size?: number;
  /** The theme token it is drawn in, default `muted-foreground`. */
  color?: IconColor;
  class?: string;
  accessibilityLabel?: string;
  testID?: string;
}

const uriCache = new Map<IconName, string>();

/** The data URI of an icon as a black template: `tintColor` colors it. */
function iconUri(name: IconName): string {
  let uri = uriCache.get(name);
  if (uri === undefined) {
    uri = `data:image/svg+xml,${encodeURIComponent(
      `<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke='${INK}' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'>${iconSources[name]}</svg>`,
    )}`;
    uriCache.set(name, uri);
  }
  return uri;
}

export function Icon(props: IconProps) {
  return (
    <Image
      src={iconUri(props.name)}
      style={{
        width: props.size ?? 16,
        height: props.size ?? 16,
        tintColor: token(props.color ?? "muted-foreground"),
      }}
      class={props.class}
      accessibilityLabel={props.accessibilityLabel}
      testID={props.testID}
    />
  );
}
