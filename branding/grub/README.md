# GRUB artwork

This directory is artwork. The image does not install it.

bootupd ships a static GRUB config. That config does not read `/etc/default/grub`, does not run `loadfont` or `set theme`, and does not look in `/usr/share/grub/themes`. A `GRUB_THEME` line and a copy under `/boot/grub2/themes` are not the boot path on this bootc image. `grub2-mkconfig` is not that path either.

`theme.txt`, `background.svg`, and the select and slider slices are the menu design if a later bootloader change can load them. `scripts/build.py` regenerates the slices. The `.pf2` fonts are not committed and are not built into the image.

Nothing here has been shown on a boot menu, because the image never loads it.
