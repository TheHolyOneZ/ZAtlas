import { Checkbox, RailLabel, Slider, TextInput } from "./ui";
import { heatGlyph, severityCss } from "../lib/severity";
import { useViewStore } from "../store/useViewStore";


const LEGEND: { swatch: "glyph" | "dash"; heat: number; label: string }[] = [
  { swatch: "glyph", heat: 0.95, label: "god file" },
  { swatch: "glyph", heat: 0.6, label: "hotspot" },
  { swatch: "glyph", heat: 0.05, label: "normal" },
  { swatch: "dash", heat: 1, label: "weak edge" },
];

export default function FilterPanel() {
  const v = useViewStore();

  return (
    <>
      <RailLabel>Filters</RailLabel>

      <div className="px-4 pb-2">
        <TextInput
          value={v.query}
          onChange={(x) => v.set("query", x)}
          placeholder="path contains…"
        />
      </div>

      <Checkbox
        checked={v.showCycles}
        onChange={(x) => v.set("showCycles", x)}
        label="Cycles"
        hint="Draw cycle and weak-evidence edges in red"
      />
      <Checkbox
        checked={v.showHotspots}
        onChange={(x) => v.set("showHotspots", x)}
        label="Hotspots"
        hint="Colour nodes by how dangerous they are, rather than uniformly"
      />
      <Checkbox
        checked={v.showOrphans}
        onChange={(x) => v.set("showOrphans", x)}
        label="Orphans only"
        hint="Show only files nothing imports"
      />
      <Checkbox
        checked={v.showTypeOnly}
        onChange={(x) => v.set("showTypeOnly", x)}
        label="Type-only imports"
        hint="Include imports erased at runtime"
      />

      <Slider
        label="minimum size"
        value={v.minLoc}
        min={0}
        max={500}
        step={10}
        onChange={(x) => v.set("minLoc", x)}
        format={(x) => (x === 0 ? "any" : `${x} LOC`)}
      />

      <RailLabel>Legend</RailLabel>
      <div className="px-4 pb-4 space-y-1.5">
        {LEGEND.map((row) => (
          <div key={row.label} className="flex items-center gap-2.5 text-[11px]">
            <span className="w-3 shrink-0 grid place-items-center">
              {row.swatch === "glyph" ? (
                <span style={{ color: severityCss(row.heat), lineHeight: 1 }}>
                  {heatGlyph(row.heat)}
                </span>
              ) : (
                <span
                  aria-hidden
                  className="w-3 border-t border-dashed"
                  style={{ borderColor: "var(--cycle)" }}
                />
              )}
            </span>
            <span style={{ color: "var(--text-3)" }}>{row.label}</span>
          </div>
        ))}
      </div>
    </>
  );
}
