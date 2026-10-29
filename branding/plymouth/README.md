# Plymouth theme

Script-module theme. The image build copies this directory to `/usr/share/plymouth/themes/darkjuander`, runs `plymouth-set-default-theme darkjuander`, then runs `dracut` so the theme replaces the initramfs the base image shipped. `bootc-image-builder` does not rebuild that initramfs later.

`/usr/lib/bootc/kargs.d/10-darkjuander.toml` adds `rhgb` and `quiet`. Without `rhgb`, Plymouth stays on the text plugin.

CI builds the image and checks `lsinitrd` for `darkjuander`, `label.so`, and `Inter.ttf`. That check has not been followed by a boot. No DARKJUANDER image has been booted.

The passphrase label uses Plymouth's `prompt_text` at Inter 14. The wordmark scales with the logo. `plymouth-plugin-script` and `plymouth-plugin-label` are both installed. A missing file fails the image build.
