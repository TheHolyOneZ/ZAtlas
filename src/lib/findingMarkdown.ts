import type { Finding } from "./tauri";


export function findingToMarkdown(finding: Finding, paths: string[]): string {
  const lines: string[] = [];

  lines.push(`**${label(String(finding.kind))}** — ${finding.headline}`);
  lines.push("");

  if (paths.length > 0) {
    lines.push(paths.map((p) => `\`${p}\``).join(paths.length > 3 ? "\n" : ", "));
    lines.push("");
  }

  if (finding.evidence.length > 0) {
    lines.push("| | |");
    lines.push("|---|---|");
    for (const e of finding.evidence) {


      lines.push(`| ${escapeCell(e.label)} | ${escapeCell(e.value)} |`);
    }
    lines.push("");
  }

  lines.push(finding.why);
  lines.push("");
  lines.push(`**Fix:** ${finding.howToFix}`);
  lines.push("");
  lines.push(`_Severity: ${finding.severity}. Found by ZAtlas._`);

  return lines.join("\n");
}

function escapeCell(text: string): string {
  return text.replace(/\|/g, "\\|").replace(/\n/g, " ");
}


function label(kind: string): string {
  const spaced = kind.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}
