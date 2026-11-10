import { expect, open, test } from "nsh/test";

test("opens the skill library from the sidebar and switches views", async () => {
  const app = await open("./App.tsx", { width: 1280, height: 800, platform: "windows" });
  await app.waitFor("id:nav-library");
  await app.press("id:nav-library");
  await app.waitFor("id:myskills-view-list");
  expect(app.count("id:myskills-view-list")).toBe(1);
  expect(app.count("id:myskills-view-card")).toBe(1);

  await app.press("id:myskills-view-card");
  await app.press("id:myskills-view-list");
  expect(app.texts().length).toBeGreaterThan(0);

  await app.press("id:nav-marketplace");
  await app.waitFor("id:nav-library");
});

test("opens the settings page from the sidebar", async () => {
  const app = await open("./App.tsx", { width: 1280, height: 800, platform: "windows" });
  await app.waitFor("id:nav-settings");
  await app.press("id:nav-settings");
  await app.waitFor("id:settings-save-status");

  expect(app.count("id:settings-language-en")).toBe(1);
  expect(app.count("id:settings-theme-dark")).toBe(1);
  expect(app.count("id:settings-proxy-custom")).toBe(1);
  expect(app.count("id:settings-test-connectivity")).toBe(1);

  await app.press("id:settings-proxy-system");
  expect(app.shows("id:settings-proxy-url")).toBe(false);

  await app.press("id:settings-proxy-custom");
  await app.waitFor("id:settings-proxy-url");
});
