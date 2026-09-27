---
description: What stretto's files hold, what leaves the machine, how to keep less with retention, and how to share sessions pseudonymized with stretto redact.
---

# Privacy and redaction

A deployment records what its tools read and return, and what its users say. This page is the short version: what to protect, how to send nothing off the machine, how to keep less, and how to share. [Privacy: files and data](/reference/privacy) is the full inventory.

## What to protect

| File | Holds |
|---|---|
| Session logs (`--record`) | Every tool call and result verbatim, and with `--context` the conversation. **As sensitive as the data the tools touch.** |
| Flow logs (`<session>.flow.jsonl`) | Each decision, with the arguments of the lookups made, such as ids. No results or messages. |
| A flow (`*.flow.json`) | Tool and argument names, JSON paths, counts, the tools' documentation. Its code features (`map.ids`) hold values copied from training results. |
| The answer cache (`--oracle-cache`) | Answers of the System-One model, never the questions' state. |

Keep logs where the data may live, and never commit them.

## What leaves the machine

The proxy talks to the MCP server it fronts, and to nothing else unless a System-One model is asked. Questions go to Jev, only on a cache miss, and only when:

- the proxy serves a flow with its arbiter (`--flow-decider arbiter`, the default) or runs the confirmation judge (`--confirm-judge`), with `--oracle jev`; or
- an offline command asks: `stretto learn` or `compile` fitting an arbiter, `phase0 --oracle`, `confirm`, `match`, `ask`.

A question carries the customer's first message and up to four later ones, the agent's last message, up to 16 earlier calls and the last four results in full, each cut to a length ([the fields](/reference/privacy#what-leaves-the-machine)).

**To send nothing, serve with `--flow-decider reach` or `habit`, learn with `--habit-only`, and leave the confirmation judge off.**

## Keep less: retention

```sh
stretto-proxy --record ~/.stretto/logs --retain-days 30 ... -- <server command>
```

When it starts, the proxy deletes the `.jsonl` and `.json` files last modified more than 30 days ago under `--record` and under `--oracle-cache`: the session logs, the flow and confirmation logs beside them, and the cached answers. It prunes at start only, and does not touch logs written elsewhere with `--flow-log` or `--confirm-log`, request dumps, or copies.

## Share: pseudonymized sessions

`stretto redact` writes a copy of recorded sessions in which rare values are replaced by salted hashes, the same hash wherever a value appears. A flow learned from the copy matches one learned from the originals.

```sh
# Keep the salt secret. Reuse it when two batches' hashes must match.
export STRETTO_REDACT_SALT="$(openssl rand -hex 16)"
stretto redact --sessions ~/.stretto/logs --out shared/logs \
  --hash-field user_id,email,name,address,payment_method_id,orders,order_id
```

- **Rare values are hashed.** A value that fewer than `--keep-shared` sessions contain (3 by default) becomes `h_` and 12 hex digits.
- **Shared values are kept.** A status or a product type says nothing about one person.
- **Named fields are always hashed.** A returning customer shares their own id among their sessions, so name the fields that identify people with `--hash-field`.
- **Tool names and schemas are kept**, so a rarely called tool's argument names survive.

Checked on the paired run's 40 retail sessions (τ²-bench's synthetic customers, with real tool outputs), the flow learned from the copy was identical to the one learned from the originals in every field but its compile time. With the named fields above, none of the 362 identifying values in those sessions (user and order ids, emails, names, addresses, payment methods) was kept ([what stays true](/reference/privacy#what-stays-true)).

::: warning Pseudonymization, not anonymization
A hash links one person's sessions to each other; that is what learning needs. Anyone who holds the salt can hash a guess and look for it. Free text in the conversation can still identify someone. Only session logs are rewritten: do not share flow logs, confirmation logs, the answer cache or request dumps.
:::

## Credentials

Pass a server's credentials in the host's `env` block, not as arguments. The proxy never records its environment, and replaces the values of credential-looking arguments in the log's header with `<redacted>` only as a best effort. For Streamable HTTP servers, `--upstream-header NAME=VAR` sends a header's value from an environment variable and never logs it ([Streamable HTTP](/integrations/streamable-http)).

## Related

- [Privacy: files and data](/reference/privacy): every file, every field sent, and the caveats
- [`stretto redact`](/reference/cli#stretto-redact)
