import { expect, open, test } from "nsh/test";

const windowOptions = { width: 1280, height: 800, platform: "windows" } as const;

test("switches the library between its views", async () => {
  const app = await open("./App.tsx", windowOptions);
  await app.waitFor("id:nav-library");
  await app.press("id:nav-library");
  await app.waitFor("id:library-search");

  await app.press("id:library-view-card");
  await app.press("id:library-view-list");
  expect(app.count("id:library-view-list")).toBe(1);
});

test("shows one page at a time", async () => {
  const app = await open("./App.tsx", windowOptions);
  await app.waitFor("id:nav-marketplace");
  await app.press("id:nav-marketplace");
  await app.waitFor("id:marketplace-search");
  expect(app.shows("id:library-search")).toBe(false);

  await app.press("id:nav-library");
  expect(app.shows("id:library-search")).toBe(true);
  expect(app.shows("id:marketplace-search")).toBe(false);
});

test("switches the download page between its three tabs", async () => {
  const app = await open("./App.tsx", windowOptions);
  await app.waitFor("id:nav-marketplace");
  await app.press("id:nav-marketplace");
  await app.waitFor("id:marketplace-tab-square");

  await app.press("id:marketplace-tab-local");
  await app.waitFor("id:import-local-drop");
  expect(app.shows("id:import-local-folder")).toBe(true);

  await app.press("id:marketplace-tab-url");
  await app.waitFor("id:import-url-unavailable");
  expect(app.shows("id:import-local-drop")).toBe(false);
});

test("offers the settings that exist and the proxy address only for a custom proxy", async () => {
  const app = await open("./App.tsx", windowOptions);
  await app.waitFor("id:nav-settings");
  await app.press("id:nav-settings");
  await app.waitFor("id:settings-language-zh_cn");

  for (const id of [
    "settings-language-zh_cn",
    "settings-language-en",
    "settings-theme-system",
    "settings-theme-light",
    "settings-theme-dark",
    "settings-proxy-system",
    "settings-proxy-none",
    "settings-proxy-custom",
    "settings-test-connectivity",
  ]) {
    expect(app.count(`id:${id}`)).toBe(1);
  }

  // Tests run on the real data directory: only press what leaves the saved settings as they are.
  await app.press("id:settings-proxy-system");
  expect(app.shows("id:settings-proxy-url")).toBe(false);
  await app.press("id:settings-proxy-custom");
  await app.waitFor("id:settings-proxy-url");
});
