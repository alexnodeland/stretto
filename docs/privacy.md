# Privacy

A deployment records what its tools read and return, and what its customers say. This page lists what each file stretto writes holds, what is sent off the machine, and how to keep less of it: retention, and pseudonymized copies to share.

## What each file holds

| File | Written by | What it holds |
|---|---|---|
| Session log, `<dir>/<session>.jsonl` | `stretto-proxy --record <dir>` | A header: the session id, start time, domain, agent model, and the server's command with credential-looking arguments replaced by `<redacted>`. Then every JSON-RPC message in both directions, verbatim: `tools/list`, each tool call's arguments and result, notifications and errors. With `--context`, the conversation, as `context` entries. The proxy's own lookups, as `proxy` entries. **Everything the tools read or return, and everything the customer said.** |
| Flow log, `<session>.flow.jsonl` beside it, or `--flow-log` | `stretto-proxy --flow` | One line per decision: the site, what the flow did (a lookup's tool and arguments, or why it handed back), the arbiter's probabilities, the System-One model's answer and the predicates', the answer's cache key, and timing. **Argument values, such as ids.** Then one line per run: the tool called before it, and each decision and outcome as a number or a yes/no with its score. No results or messages. |
| Confirmation log, `<session>.confirm.jsonl` beside it, or `--confirm-log` | `stretto-proxy --confirm-judge` | One line per judged write: the tool and its full arguments, the word list's verdict, the judge's probabilities, cache keys and timing. **A write's arguments,** such as an address or a payment method. |
| Answer cache, `<dir>/<ab>/<key>.json` | `--oracle-cache` (the proxy's default is `~/.stretto/oracle-cache`) | One file per question asked, named by the SHA-256 of the whole request: the model version that answered, the answers (a probability for each option) and token usage. **Never the request's state.** The live flow's options are tool names. The offline argument questions (`stretto phase0 --oracle`) have argument values as options, so their answers hold those values. |
| Request dump, `--oracle-dump <file>` | `stretto` commands that ask questions | Every request in full, state included. **As sensitive as the session logs.** It exists for inspection; don't keep or share it. |
| Flow, `*.flow.json` | `stretto compile`, `stretto learn` | Tool names and their documentation, argument names, JSON paths, counts and question texts. **`map.ids` holds values copied from training outputs** ([formats](formats.md)); `compile --no-features` leaves it empty. `flow-show` counts them but doesn't print them. |
| Arbiter, `*.arbiter.json` | `stretto export-arbiter`, `fit-arbiter` | Weights, the predicates' texts, the model's name and the flow it came from. No session data. |
| Answer bundle, `answers-*.jsonl.gz` | `stretto export-answers` | Request keys and answers, as in the cache. No states. |

## What leaves the machine

The logs, the cache and the flows stay where they are written. The proxy talks to the MCP server it fronts (the command after `--`, or the `--upstream` URL) and, for questions, to the System-One model. Nothing else.

Questions go to Jev (TypeSafe's `POST /v1/systemone`) only on a cache miss, and only when:

- the proxy serves a flow with its arbiter (`--flow-decider arbiter`, the default; `--flow-decider habit` asks nothing), or runs the confirmation judge (`--confirm-judge`), with `--oracle jev`; or
- an offline command asks: `stretto phase0 --oracle`, `learn` or `compile` fitting an arbiter, `confirm`, `match`, `ask`.

A flow's question at a site carries this state:

| Field | What | At most |
|---|---|---|
| `customer_request` | The customer's first message | 1,000 characters |
| `later_customer_messages` | Up to four of the customer's latest messages after it | 600 characters each |
| `agent_last_message` | The agent's last message | 600 characters |
| `earlier_lookups` | Up to 16 earlier tool calls, one line each: the tool, its arguments, and whether it failed | 160 characters of arguments each |
| `latest_results` | The last four tool calls in full: the tool, its arguments and its result | 1,500 characters of result each |
| `flow_goal` | The goal a flow was compiled for, when it has one. Live flows are goal free and leave it out | |

The questions themselves carry the tools' descriptions from the server's `tools/list`, what each lookup's results supplied in training (argument names, not values), and the predicates' texts.

The confirmation judge's question carries the end of the agent's last message (`agent_said_last`, 1,500 characters), the start of the customer's reply (`customer_replied`, 600 characters), and the call about to be made: the tool and its full arguments.

Offline, `phase0` sends the same state for recorded episodes. Its argument questions add the pending call's other arguments, with the candidate values as options.

What TypeSafe keeps of a request is governed by your agreement with TypeSafe, not by stretto. To send nothing, serve flows with the habit alone and leave the confirmation judge off. Leaving fields out of the state is not built yet ([#24](https://github.com/alexnodeland/stretto/issues/24)).

## Keeping less: `--retain-days`

`stretto-proxy --retain-days N` deletes, when it starts, the `.jsonl` and `.json` files last modified more than N days ago under `--record` and under `--oracle-cache`. That covers the session logs, the flow and confirmation logs kept beside them, and the cached answers.

It does not cover:

- flow and confirmation logs written elsewhere with `--flow-log` or `--confirm-log`;
- `--oracle-dump` files;
- copies a host or a person made.

It prunes at start only, so a proxy that runs for weeks prunes at its next start. A deleted answer is paid for again if the question comes back.

## Sharing sessions: `stretto redact`

`stretto redact` writes a pseudonymized copy of recorded sessions, from which `stretto learn` learns the same flow:

```sh
# Keep the salt secret. Reuse it when two batches' hashes must match.
export STRETTO_REDACT_SALT="$(openssl rand -hex 16)"
stretto redact --sessions ~/.stretto/logs --out shared/logs \
  --hash-field user_id,email,name,address,payment_method_id,orders,order_id
```

What it rewrites:

- the arguments of every tool call (the agent's and the proxy's), and their results: JSON keys and values, or each line of a result that is not JSON;
- the conversation (`context` entries), word by word;
- the server's notifications, and the text of error responses.

What it keeps: tool names, JSON structure, `initialize` and `tools/list`, timings, and the header, whose server command the proxy already redacted.

The rule:

- **Rare values are hashed.** A value that fewer than `--keep-shared` sessions contain (3 by default) becomes `h_` and 12 hex digits of SHA-256 of the salt and the value. It is the same hash wherever the value appears, in any of the sessions.
- **Shared values are kept.** A status, a reason, a product type or a field name appears in many sessions and says nothing about one person.
- **Named fields are always hashed.** A value that many sessions share is not always shared by many people: a customer with several sessions shares their own id among them. `--hash-field` names fields (JSON keys, at any depth of the arguments and results) whose values, and each word of them, are hashed however many sessions share them.
- **Declared names are always kept.** The tools' names, and the property names and enum values of their schemas in `tools/list`, are kept even when few sessions use them, so a rarely called tool's argument names survive.
- **Values are compared, not matched as typed.** Comparison is by value, trimmed, lower-cased and without a leading `#`, so `#W1` in a record and `w1` typed by the customer get one hash. In running text (the conversation, error messages) a value of several words from the records is matched whole, so "the desk lamp" the customer typed contains what became of "Desk Lamp" in a result.

### What stays true

Learning needs three things: which lookup followed which call, where each argument's value appeared in an earlier result, and whether the customer had mentioned it. Each value hashes the same everywhere, so all three survive. Checked:

- **Synthetic sessions.** The proxy's loop test redacts the 40 sessions it records and learns a flow from each copy. `flow-show` renders both flows the same, and `flow-diff` finds nothing to review.
- **Real tool outputs.** The paired run's 40 retail sessions without a flow were redacted, with and without the named fields above. In both cases, the flow learned from the copy is identical to the one learned from the originals in every field but its compile time: its 11 sites, the bindings of the 4 lookups it makes, and its code feature.
- **The conversation.** The cold start's 5 recorded sessions carry the conversation. With and without named fields, `flow-show` renders the flows learned from the copy and from the originals the same, including which bound values the customer had mentioned.

What survives, in those 40 retail sessions:

| | Values | Kept at `--keep-shared 3` | Kept with the named fields |
|---|---|---|---|
| User ids | 30 | 2 | 0 |
| Emails | 30 | 0 | 0 |
| First and last names | 36 | 14 | 0 |
| Street address lines | 67 | 1 | 0 |
| Cities, states, zip codes | 61 | 12 | 0 |
| Payment methods | 45 | 2 | 0 |
| Order ids | 93 | 4 | 0 |
| **All** | **362** | **35** | **0** |

The kept ids and payment methods belong to customers with three or more sessions: τ²-bench reuses customers across tasks, as a returning customer would. Name the fields that identify people in your tools' outputs, and set `--keep-shared` above the number of sessions one customer is likely to have.

### Caveats

- **Pseudonymization, not anonymization.** A hash links one person's sessions to each other; that is what learning needs. Anyone who holds the salt can hash a guess and look for it, so keep the salt secret. Free text can still identify someone, for example a unique story in the conversation.
- **Keys can be data.** Some tools key records by id, such as a customer's payment methods, so JSON keys are hashed like values. The keys of a tool called in fewer than `--keep-shared` sessions are hashed too. A binding whose path runs through such a key would differ. Before relying on a copy, learn from both and compare with `stretto flow-diff`.
- **Code features read values.** A code feature over a named field's values would hold hashes, which never match a live value. Name only fields that identify people.
- **Only session logs are rewritten.** Flow and confirmation logs, the answer cache and request dumps are not. Don't share them.

The pilots' published episodes ([episodes](results/episodes-2026-09-24.md)) hold τ²-bench's synthetic customers only.
