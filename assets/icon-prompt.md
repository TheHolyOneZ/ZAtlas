# ZAtlas icon

**The icon is done.** `assets/icon-source.png` is the source of truth, and every
platform size in `src-tauri/icons/` is generated from it.

To regenerate after changing the source:

```sh
pnpm tauri icon assets/icon-source.png
cp src-tauri/icons/128x128@2x.png public/icon.png   # the in-app titlebar
```

Mobile output is deleted after generating: Linux and Windows are the targets,
and forty Android/iOS assets are dead weight in the repository.

## Known flaws in the current icon

Worth fixing if it is ever revised. Neither blocks anything:

1. **The Z has a double outline** — a bright orange stroke offset from a muddier
   ochre fill, which reads as misregistered printing rather than a deliberate
   choice. Visible at 64px and up.
2. **The contour rings are too low-contrast** to earn their place: mush at
   1024px, invisible below 48px.

It passes the test that matters — at 32px the amber Z stays legible with the
teal network reading as an accent ring, and at 16px it reduces cleanly to an
amber Z on dark, which still ties it to the Z family.

---

# The original generation prompt

Kept for regenerating or iterating. Paste into Gemini (or any image model). Ask
for **1024×1024, PNG, transparent background**. Generate 4 variations, pick one,
then run:

```sh
pnpm tauri icon assets/icon-source.png
```

---

## The prompt

> A modern desktop application icon, 1024×1024, flat vector style, rendered on a
> transparent background.
>
> **Subject:** a stylised topographic contour map that resolves, on closer look,
> into a node-and-edge network diagram. Three or four concentric contour lines
> form the base — smooth, organic, irregular closed curves like elevation rings
> on a survey map. Sitting on those contour lines are 5–7 small filled circles
> (nodes), connected to each other by thin straight lines (edges) that cut
> across the curves. One node is noticeably larger than the others and sits
> slightly off-centre, as the focal point.
>
> **Composition:** centred, radially balanced, with generous empty margin around
> the artwork — the shape should occupy about 76% of the canvas so it stays
> legible when scaled down. Single unified silhouette, no floating detached
> fragments near the edges.
>
> **Colour:** a strictly limited palette. Deep near-black teal background shape
> (#0B1211) if a container is used, contour lines in a muted desaturated teal
> (#1E3A36), and the nodes and edges in a bright vivid teal (#2DD4BF). The one
> large focal node is warm amber (#F59E0B) — the single point of contrast in the
> whole icon. No other hues.
>
> **Style:** flat 2D vector. No gradients, no drop shadows, no bevels, no
> gloss, no 3D perspective, no skeuomorphism, no glow or bloom. Crisp geometric
> line work with uniform stroke weights — contour lines thin, edges thinner,
> nodes solid filled. The feel is a precision instrument: a surveyor's map, an
> engineering schematic, something calm and exact rather than playful.
>
> **Must not include:** any text, letters, numbers, or lettering of any kind. No
> magnifying glass. No globe or planet. No folder. No compass rose. No pin or
> location marker. No human figures. No cartoon or mascot styling. No
> photorealism.
>
> **Legibility requirement:** the silhouette must remain readable at 32×32
> pixels. Keep the number of distinct elements low, strokes thick enough to
> survive downscaling, and maintain strong contrast between the bright teal
> marks and the dark ground.

---

## Variation lines

Append **one** of these to the prompt above to steer the result:

- *Rounded-square container:* "Place the artwork inside a rounded-square app
  tile with a 22% corner radius, filled with the deep near-black teal."
- *No container:* "No background container — the contour-and-network shape
  itself is the whole icon, on transparency."
- *Hexagonal:* "Place the artwork inside a flat-top hexagon filled with the deep
  near-black teal."

## Why these constraints

- **Contours + nodes** is the product in one image: it reads as a map from
  across the room and as a dependency graph up close. That double reading is the
  identity.
- **One amber node** matches the severity ramp — teal is normal, amber is a
  hotspot. The icon states the app's colour language before you open it.
- **Flat, no effects** because it sits in a dock next to 40 other icons; gloss
  and shadow are what make an icon look dated within a year.
- **No text and no magnifying glass** because both are what every generic
  "analysis tool" icon does, and neither survives 32 px.

## After generating

Save the chosen image as `assets/icon-source.png`, then run the two commands at
the top of this file.
