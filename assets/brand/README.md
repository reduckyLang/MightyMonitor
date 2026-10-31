# Trinix: app icon

The mark is a green X on a pale green rounded square (`#EEF7EB`).

`trinix-master.png` (1024x1024, transparent surround) is the production master.
Exports generated from it:

- `assets/trinix.png` (512): desktop window icon and Linux packaging
- `website/dist/assets/icon.png` (256): website logo and favicon
- `packaging/windows/trinix.ico`: 16/20/24/32/40/48/64/128/256
- `android/app/src/main/res/mipmap-*/`: legacy `ic_launcher` and
  `ic_launcher_round` (48dp) and the adaptive foreground (108dp, mark inside
  the 66dp safe zone); the adaptive background is the colour resource
  `ic_launcher_background`

Sizes of 32 px and below are downsampled from a supersampled render and lightly
sharpened. Keep all exports synchronized when changing the master.
