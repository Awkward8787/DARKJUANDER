# Plymouth theme

Script-module theme. The image build copies this directory to `/usr/share/plymouth/themes/darkjuander`, runs `plymouth-set-default-theme darkjuander`, then runs `dracut` so the theme replaces the initramfs the base image shipped. `bootc-image-builder` does not rebuild that initramfs later.

`/usr/lib/bootc/kargs.d/10-darkjuander.toml` adds `rhgb` and `quiet`. Without `rhgb`, Plymouth stays on the text plugin.

CI runs `image/check-initramfs.sh`. That script requires the listing line `usr/share/plymouth/themes/darkjuander/darkjuander.script`, the line `Theme=darkjuander` from `lsinitrd -f etc/plymouth/plymouthd.conf`, and `usr/share/fonts/Plymouth.ttf` pointing at Inter. It also fails if the initramfs is over 140 MiB. It does not treat a match on the string `darkjuander` as proof. That check has not been followed by a boot. No DARKJUANDER image has been booted.

The passphrase label asks for Inter 14. label-freetype ignores the family name and uses `/usr/share/fonts/Plymouth.ttf`, which the image build points at Inter by installing `50-darkjuander-inter.conf` ahead of Noto. The wordmark scales with the logo. `plymouth-plugin-script` and `plymouth-plugin-label` (`label-freetype.so`) are both installed. A missing file fails the image build.

`UseSimpledrm=1` is written into `plymouthd.defaults` before dracut. The plymouth dracut module reads that key, not `/etc`, when hostonly is off. simpledrm is built into the Fedora kernel (`CONFIG_DRM_SIMPLEDRM=y`). A UEFI firmware framebuffer, which UTM provides, is enough for the splash. The full `drm` module was pulling in about 100 MB of nvidia firmware. This choice has not been booted.
