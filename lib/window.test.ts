import { expect, test } from "nsh/test";
import { primaryWorkArea, startingWindowBounds, type WindowDisplay } from "./window";

function display(primary: boolean, x: number, y: number, width: number, height: number): WindowDisplay {
  return { primary, visibleBounds: { origin: { x, y }, size: { width, height } } };
}

test("uses the primary display's work area or the first one", () => {
  expect(primaryWorkArea([])).toBe(null);
  const first = display(false, 0, 0, 1280, 720);
  const primary = display(true, 1920, 0, 1920, 1040);
  expect(primaryWorkArea([first, primary])).toBe(primary.visibleBounds);
  expect(primaryWorkArea([first])).toBe(first.visibleBounds);
});

test("centers the starting window in a large work area", () => {
  const bounds = startingWindowBounds({ origin: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } });
  expect(bounds?.size.width).toBe(1200);
  expect(bounds?.size.height).toBe(760);
  expect(bounds?.origin.x).toBe(360);
  expect(bounds?.origin.y).toBe(140);
});

test("centers relative to a secondary display's origin", () => {
  const bounds = startingWindowBounds({ origin: { x: 1920, y: 100 }, size: { width: 1600, height: 900 } });
  expect(bounds?.origin.x).toBe(2120);
  expect(bounds?.origin.y).toBe(170);
});

test("shrinks the window on small displays and keeps a positive size", () => {
  const bounds = startingWindowBounds({ origin: { x: 0, y: 0 }, size: { width: 1024, height: 600 } });
  expect(bounds?.size.width).toBe(976);
  expect(bounds?.size.height).toBe(552);
  expect(bounds?.origin.x).toBe(24);
  expect(bounds?.origin.y).toBe(24);

  const tiny = startingWindowBounds({ origin: { x: 0, y: 0 }, size: { width: 20, height: 20 } });
  expect(tiny?.size.width).toBe(1);
  expect(tiny?.size.height).toBe(1);
});

test("skips placement when no display is available", () => {
  expect(startingWindowBounds(null)).toBe(null);
  expect(startingWindowBounds(undefined)).toBe(null);
});
