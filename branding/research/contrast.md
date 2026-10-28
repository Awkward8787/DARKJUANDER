# DARKJUANDER contrast ratios

Computed by `scripts/contrast.py` with the WCAG 2.x relative-luminance formula. 4.5:1 is the AA minimum for body text, 3:1 for large text and UI parts, 7:1 is AAA.

| Pair | Foreground | Background | Ratio | Grade |
|---|---|---|---|---|
| Body text: lumen_100 on umbra_950 | `#E6E9F0` | `#0A0C11` | 16.09:1 | AAA |
| Body text: lumen_100 on umbra_900 | `#E6E9F0` | `#11141B` | 15.16:1 | AAA |
| Muted text: ash_400 on umbra_950 | `#8A93A6` | `#0A0C11` | 6.34:1 | AA |
| Muted text: ash_400 on umbra_800 | `#8A93A6` | `#1B2029` | 5.29:1 | AA |
| Accent: violet on umbra_950 | `#9D8CFF` | `#0A0C11` | 7.11:1 | AAA |
| Accent: violet on umbra_800 | `#9D8CFF` | `#1B2029` | 5.94:1 | AA |
| Button label: umbra_950 on violet | `#0A0C11` | `#9D8CFF` | 7.11:1 | AAA |
| Light mode text: umbra_950 on lumen_50 | `#0A0C11` | `#F4F5F8` | 17.94:1 | AAA |
| Light mode text: umbra_950 on white | `#0A0C11` | `#FFFFFF` | 19.56:1 | AAA |
| Light mode accent: violet_deep on white | `#5B48D6` | `#FFFFFF` | 6.28:1 | AA |
| Light mode accent: violet_deep on lumen_50 | `#5B48D6` | `#F4F5F8` | 5.76:1 | AA |
| Button label: white on violet_deep | `#FFFFFF` | `#5B48D6` | 6.28:1 | AA |
| Divider: umbra_600 on umbra_950 (decorative only) | `#3A4252` | `#0A0C11` | 1.94:1 | fail |
| Input border: umbra_500 on umbra_950 (UI part) | `#5A6377` | `#0A0C11` | 3.25:1 | AA large / UI |
| Dark semantic ok on umbra_950 | `#3DD68C` | `#0A0C11` | 10.43:1 | AAA |
| Dark semantic ok on umbra_800 | `#3DD68C` | `#1B2029` | 8.71:1 | AAA |
| Dark semantic warn on umbra_950 | `#F5B841` | `#0A0C11` | 11.01:1 | AAA |
| Dark semantic warn on umbra_800 | `#F5B841` | `#1B2029` | 9.19:1 | AAA |
| Dark semantic danger on umbra_950 | `#FF5C5C` | `#0A0C11` | 6.46:1 | AA |
| Dark semantic danger on umbra_800 | `#FF5C5C` | `#1B2029` | 5.40:1 | AA |
| Dark semantic info on umbra_950 | `#4CB8FF` | `#0A0C11` | 8.95:1 | AAA |
| Dark semantic info on umbra_800 | `#4CB8FF` | `#1B2029` | 7.48:1 | AAA |
| Light semantic ok on white | `#0E7A47` | `#FFFFFF` | 5.39:1 | AA |
| Light semantic warn on white | `#8A5A00` | `#FFFFFF` | 5.93:1 | AA |
| Light semantic danger on white | `#C22B2B` | `#FFFFFF` | 5.72:1 | AA |
| Light semantic info on white | `#0B6BB3` | `#FFFFFF` | 5.57:1 | AA |
| Terminal  0 black on background | `#11141B` | `#0A0C11` | 1.06:1 | fail |
| Terminal  1 red on background | `#FF5C5C` | `#0A0C11` | 6.46:1 | AA |
| Terminal  2 green on background | `#3DD68C` | `#0A0C11` | 10.43:1 | AAA |
| Terminal  3 yellow on background | `#F5B841` | `#0A0C11` | 11.01:1 | AAA |
| Terminal  4 blue on background | `#4CB8FF` | `#0A0C11` | 8.95:1 | AAA |
| Terminal  5 magenta on background | `#9D8CFF` | `#0A0C11` | 7.11:1 | AAA |
| Terminal  6 cyan on background | `#3ED6D0` | `#0A0C11` | 10.93:1 | AAA |
| Terminal  7 white on background | `#C3C9D6` | `#0A0C11` | 11.78:1 | AAA |
| Terminal  8 bright black on background | `#7A8398` | `#0A0C11` | 5.15:1 | AA |
| Terminal  9 bright red on background | `#FF8585` | `#0A0C11` | 8.33:1 | AAA |
| Terminal 10 bright green on background | `#6FE6AA` | `#0A0C11` | 12.64:1 | AAA |
| Terminal 11 bright yellow on background | `#FFD27A` | `#0A0C11` | 13.74:1 | AAA |
| Terminal 12 bright blue on background | `#82CEFF` | `#0A0C11` | 11.38:1 | AAA |
| Terminal 13 bright magenta on background | `#BDB2FF` | `#0A0C11` | 10.22:1 | AAA |
| Terminal 14 bright cyan on background | `#7FE8E3` | `#0A0C11` | 13.56:1 | AAA |
| Terminal 15 bright white on background | `#F4F5F8` | `#0A0C11` | 17.94:1 | AAA |
| Terminal foreground on background | `#E6E9F0` | `#0A0C11` | 16.09:1 | AAA |
| Terminal selection_fg on selection_bg | `#F4F5F8` | `#2A2F3D` | 12.25:1 | AAA |
