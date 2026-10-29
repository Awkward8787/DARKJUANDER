# Cloud providers

UMBRA can route a request to a cloud model. The image default does not. A
provider is off until an operator stores a credential and grants that
provider, and a caller still needs `provider.use:<id>` plus `model.use:<model>`.
Private and secret labels never leave the machine, grant or not.

The client is one OpenAI-compatible adapter (`openai-compatible` in
`umbrad.yaml`). xAI and OpenAI are presets. It speaks chat completions. A
ChatGPT subscription credential uses the Responses API instead, because that
is the documented contract for plan usage.

Checked against the provider docs on 2026-10-28:

| Provider | Base URL | Default wire |
|---|---|---|
| xAI | `https://api.x.ai/v1` | `POST /chat/completions` |
| OpenAI | `https://api.openai.com/v1` | `POST /chat/completions` |

xAI documents chat completions as a legacy OpenAI-compatible endpoint and
sends newer features to its Responses API first. This adapter uses chat
completions for API keys. Sources:
[xAI quickstart](https://docs.x.ai/developers/quickstart),
[chat completions](https://docs.x.ai/developers/model-capabilities/legacy/chat-completions),
[inference reference](https://docs.x.ai/developers/rest-api-reference/inference).

OpenAI chat completions:
[chat reference](https://developers.openai.com/api/reference/resources/chat).

## What can leave the machine

A cloud call sends the role and content of the messages, and the model id.
It does not send DARKJUANDER labels.

That happens only when all of the following are true:

1. `local_only` is false for the daemon and the caller.
2. The provider is granted (`dj provider grant`, or `providers.<id>.granted: true`).
3. The caller holds `provider.use:<id>` and `model.use:<selected model>`.
4. The joined confidentiality label is `public`.

`private` and `secret` are removed by a router rule in code, before the
scorer and before any socket. The shipped policy says the same thing. Editing
the policy file does not put those labels back on a cloud route. The broker
checks the resolved route again, and the adapter refuses a request that is
not explicitly public.

The audit record stores the provider id, the model id, the route, and token
counts. It does not store the API key, the access token, or the prompt text.
`dj provider test` sends the fixed public string `ping` and nothing else.
`--base-url` may name the provider's preset host (`api.x.ai` or
`api.openai.com`) or a loopback address. Any other host is refused, so the
bearer key is not sent to an arbitrary https server. `--allow-untrusted-host`
lifts that host check and nothing else: a cleartext URL that is not loopback
is still refused, and a URL with embedded credentials is still refused.

Cloud output is labeled untrusted, joined with the input labels.

`umbrad.service` still sets `IPAddressAllow=localhost` and `IPAddressDeny=any`.
A grant does not open egress. Prefer to leave `umbrad` on localhost and give
outbound access only to a proxy that dials the providers.

```
# Recommended. umbrad stays localhost-only. This unit is a forward proxy
# you run; DARKJUANDER does not ship that binary.
# /etc/systemd/system/dj-provider-proxy.service
[Service]
ExecStart=/usr/bin/your-forward-proxy --listen 127.0.0.1:8080
IPAddressAllow=localhost
IPAddressAllow=api.x.ai
IPAddressAllow=api.openai.com
IPAddressDeny=any
```

Point the engine `base_url` at `http://127.0.0.1:8080/v1`. The proxy should
connect only to those two names. systemd resolves `IPAddressAllow` hostnames
when the unit is loaded, so restart the proxy after the addresses change.
If you pin prefixes instead, use ranges you have checked yourself.
`192.0.2.0/24` and `2001:db8::/32` are documentation ranges, not the
providers' networks.

```
# Broader, and not the one to copy first.
# /etc/systemd/system/umbrad.service.d/cloud-egress.conf
[Service]
IPAddressAllow=any
```

`IPAddressAllow=any` lets the daemon reach every host. `systemctl daemon-reload`
and restart the unit after either drop-in.

## Add a key

The key is not a command-line flag. Pass a file or stdin so it stays out of
the process list.

```bash
dj provider add xai --key-file ./xai.key --model MODEL
dj provider add openai --key-file ./openai.key --model MODEL
dj provider grant xai
dj provider grant openai
dj provider list
dj provider test xai
dj provider remove xai
```

`MODEL` is an id from that provider's model list, not a DARKJUANDER alias.
The xAI chat-completions example used `grok-4.7` on 2026-10-28. Confirm the
id in the console before you grant a route. `grant` is the explicit switch.
`add` leaves `granted: false`.

Keys land in `{state}/credentials/<id>.key` mode `0600`, and the parent
directory mode `0700`. The default state directory is
`/var/lib/darkjuander` (`DJ_STATE_DIR` or `--state-dir` overrides it). The
registry at `{state}/providers/registry.yaml` records the provider and the
path. It does not contain the key.

Point the daemon at that registry and turn local-only off:

```yaml
provider_registry: /var/lib/darkjuander/providers/registry.yaml
local_only: false
```

A granted registry entry adds a cloud model named by `default_model` when
that id is not already in the config. The caller still needs:

```yaml
grants:
  - "provider.use:xai"
  - "model.use:MODEL"
```

`prefer_cloud` only ranks a cloud model the hard rules already allowed.

On the image, run `dj provider add` as root and then either:

- `chown dj-umbra:dj-umbra` the key file so the daemon user can read it, or
- pass `--systemd` and load the root-owned `0600` file with `LoadCredential`.

```
# /etc/systemd/system/umbrad.service.d/credentials.conf
[Service]
LoadCredential=dj-provider-xai:/var/lib/darkjuander/credentials/xai.key
LoadCredential=dj-provider-openai:/var/lib/darkjuander/credentials/openai.key
```

systemd exposes those files under `CREDENTIALS_DIRECTORY`. The daemon reads
that directory and does not log the bytes. A mode other than owner-read
(`0600` or `0400`) is refused.

The kernel keyring is not a store here. A user keyring does not survive
reboot for this daemon without a separate helper, and reading it would need
raw syscalls. The file and `LoadCredential` cover the two persistent cases.

## Sign-in and subscriptions

API keys and consumer subscriptions are different products.

xAI's developer docs authenticate with an API key from the console
([quickstart](https://docs.x.ai/developers/quickstart)). They do not document
an OAuth or device flow that lets a third-party app spend a SuperGrok or
grok.com subscription. Other clients describe a login against
`accounts.x.ai`. That is not in the xAI docs, so `dj provider login xai`
refuses it and does not call those endpoints.

OpenAI's API is also key-based and billed on the API platform, separate from
ChatGPT Plus, Pro, or a similar plan. OpenAI does document a second path for
open-source, locally hosted apps: Sign in with ChatGPT. Eligible plans can
grant `chatgpt.tokens.use.direct` for the Responses API. It is not a chat
completions key, and it does not reveal the user's API key or ChatGPT
history.

- [Quickstart](https://developers.openai.com/siwc/quickstart)
- [Open-source plan usage](https://developers.openai.com/siwc/token-sharing-open-source)
- [Registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
- [Models and inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)
- [Accounts, refresh, and revoke](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions)
- Discovery: <https://auth.openai.com/.well-known/openid-configuration>

`dj provider login openai` follows that public-client flow: PKCE, loopback
on `127.0.0.1`, an issued client id, RS256 ID-token checks against the
published JWKS, and scope `chatgpt.tokens.use.direct`. Inference uses
`POST /v1/responses` with `store: false` and `stream: true`. Tokens are
stored in a `0600` file. The authorize URL is printed only when it has no
`id_token_hint`. Commercial Sign in with ChatGPT is a separate limited
trial; this command does not use a partner secret.

```bash
dj provider login openai
dj provider model openai MODEL
dj provider grant openai
```

`MODEL` has to be a slug the signed-in account can use. The same label rules
apply. A subscription token is still a cloud credential.

## Cost

Each provider bills its own API account. ChatGPT plan usage, when the
subscription flow above is granted, is metered by OpenAI against that plan
instead. This tree does not copy price tables. Read the current pages:

- <https://docs.x.ai/developers/pricing>
- <https://developers.openai.com/api/docs/pricing>

Prices, model ids, and plan limits change. Confirm them in the provider
console before you grant a route.

## Failures

The adapter times out, retries `408`, `429`, and `5xx` with backoff, and
does not retry other status codes. A redirect is not followed. A cleartext
URL is refused unless the host is loopback, which is how the tests run. The
response body is not copied into the error string. A missing credential, a
bad response, or an exhausted retry fails the request. It does not fall
through to another provider.
