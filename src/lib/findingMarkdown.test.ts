import { describe, expect, it } from "vitest";

import { findingToMarkdown } from "./findingMarkdown";
import type { Finding } from "./tauri";

const BASE: Finding = {
  kind: "godFile" as Finding["kind"],
  severity: "high" as Finding["severity"],
  score: 0.9,
  headline: "src/api/handlers.ts is 2100 lines with 34 dependents",
  files: [],
  evidence: [
    { label: "Lines of code", value: "2100" },
    { label: "Files depending on it", value: "34" },
  ],
  why: "This file is large, widely depended upon and frequently changed.",
  howToFix: "Split it along the seams its dependents already imply.",
};

describe("findingToMarkdown", () => {
  it("leads with the kind and the headline", () => {
    const out = findingToMarkdown(BASE, []);
    expect(out.startsWith("**God file** — src/api/handlers.ts")).toBe(true);
  });

  it("turns a camelCase kind into words", () => {
    const out = findingToMarkdown(
      { ...BASE, kind: "layeringViolation" as Finding["kind"] },
      [],
    );
    expect(out).toContain("**Layering violation**");
  });

  it("renders the evidence as a table", () => {
    const out = findingToMarkdown(BASE, []);
    expect(out).toContain("|---|---|");
    expect(out).toContain("| Lines of code | 2100 |");
  });

  it("escapes a pipe in a value rather than shifting the table", () => {


    const out = findingToMarkdown(
      { ...BASE, evidence: [{ label: "Written as", value: "a|b" }] },
      [],
    );
    expect(out).toContain("| Written as | a\\|b |");
  });

  it("keeps the why and the fix, which are what make it actionable", () => {
    const out = findingToMarkdown(BASE, []);
    expect(out).toContain(BASE.why);
    expect(out).toContain(`**Fix:** ${BASE.howToFix}`);
  });

  it("lists paths inline when there are few and on lines when there are many", () => {
    const few = findingToMarkdown(BASE, ["a.ts", "b.ts"]);
    expect(few).toContain("`a.ts`, `b.ts`");
    const many = findingToMarkdown(BASE, ["a.ts", "b.ts", "c.ts", "d.ts"]);
    expect(many).toContain("`a.ts`\n`b.ts`");
  });

  it("omits the evidence table entirely when there is none", () => {
    const out = findingToMarkdown({ ...BASE, evidence: [] }, []);
    expect(out).not.toContain("|---|---|");
  });

  it("contains no HTML, since half the places this lands strip it", () => {
    expect(findingToMarkdown(BASE, ["a.ts"])).not.toMatch(/<[a-z]/i);
  });
});
