# Phase 0 defaults for the open questions

PRD section 14 is unanswered. These defaults are reversible. They are not a decision by Jaamal.

| Question | Default used here |
|---|---|
| 1. License of our code | Apache-2.0. |
| 2. Desktop | None in this PR. When one is required, start from the Fedora bootc GNOME base, because that is the bootc desktop with the most existing image work. |
| 3. Business model | Unset. Nothing in the tree depends on it. |
| 4. Hardware partners | VM and later install only. |
| 5. Trademark | No search was done. DARKJUANDER and UMBRA are working names. |
| 6. Apple silicon | A VM is the target. Bare metal x86_64 is the later host. Asahi is out of this PR. |
| 7. Cloud providers | None. The image config is local-only. The demo config has a mock cloud engine so the hard rule can be tested. |
| 8. Which of Jaamal's agents move first | Unknown. The scenes use synthetic agents. |
| 9. SONA and games | Not integrated. |
| 10. Security review | No review has been done and none is assumed. |
| 11. UMBRA name | Kept as the working name. |
| 12. First engine | llama.cpp. CI uses the mock adapter. |
| 13. Personal memory | Not built. It stays off until a user turns it on. |
| 14. On-device router learning | Not built. The scorer is a fixed function of pressure and model size. It stores nothing. |
| 15. Telemetry | None. Opt-in crash reporting is not implemented. The image definition masks rpm-ostree count-me when that unit exists. |
