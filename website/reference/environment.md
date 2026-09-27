---
description: The environment variables stretto reads - the optional TypeSafe key and its settings, the redaction salt, and the variables for upstream headers.
---

# Environment variables

stretto needs no environment variable to record sessions, learn flows with the habit alone, serve them with the `reach` or `habit` decider, review, audit, promote, redact, or run a procedure. The TypeSafe variables matter only for what asks a System-One model.

| Variable | Read by | What it does |
|---|---|---|
| `TYPESAFE_API_KEY` | anything that asks Jev | The key for TypeSafe's System-One model, Jev. Optional: see [when it is needed](#when-a-key-is-needed). Never logged. |
| `TYPESAFE_API_KEY_FILE` | the same | A file holding the key, read when `TYPESAFE_API_KEY` is not set. Useful when the proxy runs under an agent's process, which should not hold the key itself. |
| `TYPESAFE_BASE_URL` | the same | Where Jev is. Default `https://api.typesafe.ai`. |
| `TYPESAFE_DEFAULT_MODEL` | the same | The model id to request when `--oracle-model` is not given. Default `jev-latest`. |
| `SSL_CERT_FILE` | the same | A PEM bundle of extra root certificates for the connection to Jev, such as a proxy's. |
| `STRETTO_REDACT_SALT` | `stretto redact` | The salt for pseudonymized values. `--salt-env` names another variable. Keep it secret, and the same for batches whose hashes should match. |
| any, by name | `stretto-proxy --upstream-header NAME=VAR` | The value of the header `NAME` sent to a Streamable HTTP server, such as `ORDERS_AUTH` holding `Bearer …`. Never logged. |
| `HOME` (or `USERPROFILE`) | `stretto-proxy` | Where a leading `~` in the proxy's path options points. |
| `STRETTO_BLESS` | the test suite | Set when running `cargo test`, it rewrites the generated documentation (the [CLI reference](./cli), the examples in [reviewing flows](./review)) instead of failing on a difference. |

The wrapped server inherits the proxy's environment, so the variables a server needs go in the host's `env` block as usual. The proxy never records its environment.

## When a key is needed

A command asks Jev, and needs `TYPESAFE_API_KEY` (or `TYPESAFE_API_KEY_FILE`), only when its answers are not already in the replay cache and:

- `stretto-proxy` serves a flow with its arbiter (`--flow-decider arbiter`, the default, with a flow that has one) or runs the confirmation judge (`--confirm-judge`), with `--oracle jev` (the default);
- `stretto learn` fits an arbiter, which it does unless given `--habit-only` or `--arbiter-from`;
- `stretto serve` or `flow-serve` decides with the arbiter, with `--oracle jev` (the default of `serve`);
- `stretto phase0`, `confirm`, `ask`, `audit` or `promote` runs with `--oracle jev`, or `stretto match` runs (its default oracle is `jev`);
- `stretto jev-check`, which checks the key.

Everything else asks nothing. With `--oracle replay`, commands read the cache only; with `--oracle mock`, they check the pipeline for free.

::: tip Serving with no key
`stretto learn --habit-only` and `stretto-proxy --flow-decider reach` (or `habit`) send nothing to any model and need no key.
:::

## Related

- [Deciders](/guide/concepts/deciders): what the arbiter does with the key
- [Privacy: what leaves the machine](./privacy#what-leaves-the-machine)
