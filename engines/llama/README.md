# llama.cpp adapter

The adapter is `services/umbra/src/engine/llama.rs`. It POSTs to
`{base_url}/v1/chat/completions` and does not spawn the server.

Pinned upstream: [llama.cpp v0.8.0](https://github.com/ggml-org/llama.cpp/releases/tag/v0.8.0),
commit `8642541b255bfb5dc39cd351a178be09013a55d6`, MIT license.
`scripts/build-llama.sh` clones that tag, checks the commit, and builds
`llama-server` for the CPU.

v0.8.0 listens on `127.0.0.1:9931` unless `--port` is set. `image/systemd/dj-llama.service`
uses that port and is not enabled. Do not pass `--tools` or `--agent`.

Smoke-test weights: SmolLM2 135M Q4_K_M published by ggml-org.

- URL: `https://huggingface.co/ggml-org/SmolLM2-135M-GGUF/resolve/1076b85d135017971845b19b8d07df1479155988/SmolLM2-135M-Q4_K_M.gguf`
- SHA-256: `e3131339bf4e8065265593d4fd8f7bb7ff2d3abff1edb5618aa1197b89cad9f5`
- License: Apache-2.0
- Checked by downloading the file on 2026-10-28 and running `sha256sum`. The file was deleted and is not in git.

`scripts/fetch-model.sh` downloads it again and exits if the hash differs.
Point `umbrad.yaml` at `kind: llama-cpp` and `base_url: http://127.0.0.1:9931`
after both scripts have been run. Until then the image default stays on the mock engine.
