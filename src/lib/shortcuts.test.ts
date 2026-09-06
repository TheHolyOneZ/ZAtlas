import { describe, expect, it } from "vitest";

import {
  SHORTCUTS,
  bindingFor,
  isBareKey,
  isTypingTarget,
  shortcutLabel,
} from "./shortcuts";

function key(init: Partial<KeyboardEventInit> & { key: string }): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

describe("bindingFor", () => {
  it("folds Meta into Ctrl so one binding covers both platforms", () => {
    expect(bindingFor(key({ key: "k", ctrlKey: true }))).toBe("ctrl+k");
    expect(bindingFor(key({ key: "k", metaKey: true }))).toBe("ctrl+k");
  });

  it("orders modifiers consistently regardless of which are held", () => {
    expect(bindingFor(key({ key: "p", ctrlKey: true, shiftKey: true, altKey: true }))).toBe(
      "ctrl+shift+alt+p",
    );
  });

  it("aliases keys whose event.key does not read like the binding", () => {


    expect(bindingFor(key({ key: " " }))).toBe("space");
    expect(bindingFor(key({ key: "Escape" }))).toBe("escape");
    expect(bindingFor(key({ key: "ArrowUp" }))).toBe("up");
  });

  it("lowercases so a shifted letter still matches its binding", () => {
    expect(bindingFor(key({ key: "F" }))).toBe("f");
  });

  it("does not produce a bare binding when a modifier is held", () => {


    expect(bindingFor(key({ key: "f", ctrlKey: true }))).not.toBe("f");
  });
});

describe("isTypingTarget", () => {
  it("is true inside text entry, so single-letter shortcuts stay inert", () => {
    for (const tag of ["input", "textarea", "select"]) {
      expect(isTypingTarget(document.createElement(tag))).toBe(true);
    }
  });

  it("is true inside a contenteditable region", () => {
    const el = document.createElement("div");
    el.setAttribute("contenteditable", "true");
    expect(isTypingTarget(el)).toBe(true);
  });

  it("is false on ordinary elements and on null", () => {
    expect(isTypingTarget(document.createElement("div"))).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});

describe("the registry itself", () => {
  it("has no duplicate ids", () => {
    const ids = SHORTCUTS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("has no two shortcuts bound to the same keys", () => {
    const keys = SHORTCUTS.map((s) => s.keys);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("stores every binding in the normalised form bindingFor produces", () => {


    for (const s of SHORTCUTS) {
      expect(s.keys).toBe(s.keys.toLowerCase());
      expect(s.keys.trim()).toBe(s.keys);
    }
  });

  it("covers every shortcut the spec lists in §8", () => {
    const expected = ["ctrl+k", "1", "2", "3", "4", "f", "escape", "i", "e", "c", "/"];
    const actual = new Set(SHORTCUTS.map((s) => s.keys));
    for (const k of expected) expect(actual.has(k)).toBe(true);
  });

  it("flags the bare-key bindings that need the typing and modifier guards", () => {
    const bare = SHORTCUTS.filter((s) => isBareKey(s.keys)).map((s) => s.keys);
    expect(bare).toEqual(expect.arrayContaining(["f", "i", "e", "c", "/"]));
  });
});

describe("shortcutLabel", () => {
  it("renders a readable label", () => {
    expect(shortcutLabel("ctrl+k")).toBe("Ctrl + K");
    expect(shortcutLabel("escape")).toBe("Escape");
    expect(shortcutLabel("f")).toBe("F");
  });
});

describe("the findings walk", () => {
  it("binds the bracket keys, which need no modifier", () => {


    const next = SHORTCUTS.find((s) => s.id === "nextFinding");
    const prev = SHORTCUTS.find((s) => s.id === "prevFinding");
    expect(next?.keys).toBe("]");
    expect(prev?.keys).toBe("[");
  });

  it("matches a bare bracket but not a modified one", () => {
    expect(bindingFor(new KeyboardEvent("keydown", { key: "]" }))).toBe("]");
    expect(
      bindingFor(new KeyboardEvent("keydown", { key: "]", ctrlKey: true })),
    ).not.toBe("]");
  });
});
