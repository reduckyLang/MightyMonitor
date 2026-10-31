# Trinix: folded screens

The mark joins two screen-like upright planes into one M. Evergreen carries the
silhouette; a leaf-green fold connects the screens. It uses flat fills, shared
geometry and generous spacing so it remains legible in a taskbar.

`trinix.svg` is the editable production master. PNG exports are used by the
desktop window, Linux packaging, Android launcher and website. The Windows ICO
contains 16/24/32/48/64/128/256-pixel images. No remote asset is loaded by the app.

The built-in image-generation tool explored the initial icon and a refinement.
The first linked-frame proposal was rejected. The refined folded-screen direction
was redrawn as an original vector master to eliminate raster artifacts and make
future resizing deterministic. The generated refinement is saved locally in
`output/brand-review/refined-concept.png` (not distributed with the application).

Refinement prompt used with the built-in tool:

> Refine the supplied icon substantially. Replace the linked speech-bubble symbol
> with a simple geometric M built from two upright screen panels folding into a
> shared central valley. Use three broad flat geometric planes, exact symmetry,
> strong negative space, consistent corner radii and generous margins. Evergreen
> #184C37 with a structural leaf-green #ABD45B central plane on a pale mint tile.
> No outlined linked rectangles, chat bubbles, arrows, monitor stands, words,
> gradients, texture, bevels, shadows or glows. Single square production app icon,
> readable at 24px; restrained industrial design.

The generated concept informed the direction; the checked-in SVG geometry is the
source of truth. Keep PNG/ICO exports synchronized when changing it.
