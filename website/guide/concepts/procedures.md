---
description: Where no user speaks, compile a whole workflow once from traces, writes included, and run it with stretto-procedure, with no model and a check of the outcome.
---

# Procedures

While a user is present, stretto takes only the reads: replies, writes, and whatever the user's words decide stay with the model. Some work has no user in the loop. A ticket comes in, and the agent works through a procedure against the tools until the ticket's stated outcome holds. There every branch follows a tool result, so the environment decides every step, and a whole procedure, writes included, can be compiled once from traces and run with no model.

## Run one

```sh
stretto-procedure --procedure telecom.procedure.json --ticket "…the ticket's text…" -- <server command>
```

`stretto-procedure` starts the MCP server from the command after `--`, calls its tools one at a time as the procedure's trees say until they say stop, then checks the outcome the ticket states with a read of its own. The run goes to stdout as JSON (or to `--out FILE`): its calls, its check, and its verdict.

| Verdict | Meaning |
|---|---|
| `resolved` | The check confirmed the outcome the ticket states. |
| `transferred` | The procedure handed the customer to a person, as the policy directs. |
| `hand_back` | The check failed. Give the ticket, with the run's calls and their results, to a model. |

`--ticket-file FILE` reads the ticket from a file instead. Every option is in the [CLI reference](/reference/cli#stretto-procedure).

## The cascade

Handing back on the check makes a cascade: the procedure takes the tickets it can confirm, and the expensive model sees only the rest, starting from the procedure's state rather than from scratch. On τ²-bench's solo telecom tasks, where the agent operates the customer's phone itself:

| | Passed (40 held-out tasks) | LLM turns per ticket |
|---|---|---|
| The procedure alone | 35 of 40 | 0 |
| The procedure, then GLM-5.3 on its 4 hand-backs | 39 of 40 | 0.48 |
| GLM-5.3 alone, on 8 tickets | 6 of 8 | 15.9 |

The procedure's check handed back all four of its failures, and missed one, a run it judged resolved. Live, GLM-5.3 resolved all four hand-backs in both of two trials ([the paper, §4.5](/research/paper#45-compiling-where-no-user-speaks)).

## What a procedure is

A procedure is a file, the procedure IR (`stretto_procedure: 1`): per *site*, the tool that just returned or the start, a decision tree over the run's state, whose leaves are whole calls. It also holds the checks: for each outcome a ticket may state, the read that checks it and what its result must contain.

- **Identifiers are learned as where they came from**: the path of an earlier result that held the value, with the fields that set its record apart in a list, or the shape the value had in the ticket. They are bound again at run time from that run's results, so a procedure runs for a customer no trace saw. Renamed throughout τ²-bench's database, it passed the same 35 of 40 held-out tasks, against 16 with identifiers as constants ([results](../../../docs/results/telecom-workflow-2026-09-26.md#a-customer-no-trace-saw)).
- **A guard** forbids repeating a read before a write, or a write.
- **Nothing in it is a model weight**: trees, bindings and checks that a person can read.

[File formats](/reference/formats#the-procedure-ir-stretto_procedure-1) documents every field.

## Where one comes from

Today a procedure is compiled by a script, from τ²-bench telecom's solo runs: [`scripts/telecom_workflow.py`](../../../scripts/telecom_workflow.py) fits the trees and `--export` writes the IR. The published one, [`telecom-workflow-2026-09-26.procedure.json`](../../../docs/results/telecom-workflow-2026-09-26.procedure.json), was compiled from four runs' successful training episodes; `stretto-procedure` made exactly the calls of the script on all 40 held-out tasks, with the same rewards and verdicts.

## Limits

- **τ²-bench's own variant.** The procedure was fitted on 730 successful episodes of one customer's base tasks, carried to renamed and moved customers.
- **The check is only as good as the outcome the ticket states.** Its one missed failure shows that.
- **Many consistent demonstrations.** A procedure is cloned behavior: fitted on a quarter of the training tasks, it passed 18 of 40 ([the paper, §6](/research/paper#6-limitations)).
- **Requests still need reading.** In benchmarks without a simulated user, a procedure compiled once would need a model to read the request, and most often one to write.

## Related

- [How it works](../how-it-works#where-no-user-speaks)
- [The workflow results](../../../docs/results/telecom-workflow-2026-09-26.md)
