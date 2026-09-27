---
layout: home
title: stretto
titleTemplate: Fewer LLM turns for tool-using agents

hero:
  name: stretto
  text: Fewer LLM turns for tool-using agents
  tagline: stretto learns, from your agent's recorded tool calls, which reads come next and where their arguments come from. An MCP proxy then makes those reads for it, in the same tool result.
  image:
    src: /logo.svg
    alt: ''
  actions:
    - theme: brand
      text: Get started
      link: /guide/quick-start
    - theme: alt
      text: Why stretto?
      link: /guide/why

features:
  - title: Any MCP server, any host
    details: stretto-proxy takes the server's place in your host's configuration, and runs it as a command or reaches it over Streamable HTTP. It forwards every message and records each session.
    link: /integrations/
    linkText: Integrations
  - title: Reads only, so the worst case is a detour
    details: A flow calls only tools it learned as lookups and that the server does not mark as writes. A wrong guess costs one extra lookup, never an action.
    link: /guide/concepts/lookups
    linkText: Lookups and detours
  - title: No model of its own
    details: The reach decider scores each lookup by its chance of use before the agent's next write, counted from traces. It needs no key. Live, GLM-5.3 took 27.9% fewer LLM turns (19.1–35.9%) on 28 τ²-bench tasks.
    link: /guide/concepts/deciders
    linkText: Deciders
  - title: A flow is a file you review
    details: flow-show renders a flow for a reviewer. flow-diff lists what changed and exits with 1 when a change needs review, so a flow goes through a pull request like code.
    link: /guide/concepts/audit-and-review
    linkText: Audit and review
  - title: Shadow first, then promote
    details: In shadow mode a flow decides and logs, but looks nothing up. stretto promote then keeps it to the sites where its lookups were the agent's own.
    link: /guide/concepts/shadow-and-promotion
    linkText: Shadow mode
  - title: Compile once where no user speaks
    details: stretto-procedure runs a compiled workflow, writes included, with no model, and hands back what its outcome check cannot confirm. On τ²-bench's solo telecom tasks, 39 of 40 held-out tasks passed with a model on its 4 hand-backs.
    link: /guide/concepts/procedures
    linkText: Procedures
---

## How it works

<p class="home-lead">stretto sits where MCP already puts a server. It records what your agent does, learns which reads follow which calls, and serves those reads behind the agent's own calls.</p>

<HowItWorks />

<!-- BRAND SLOT: the animated explainer (website/public/explainer/index.html). Renders nothing until the file is there. -->
<BrandEmbed kind="explainer" />

## What the agent sees

The agent searches a folder of notes for the October meetings. The flow reads both meetings behind that one call, and the agent gets its own result with one more text item:

```text
--- Also looked up automatically (current results; no need to repeat these calls) ---

read_text_file {"path":"/home/me/notes/meetings/2026-10-06.md"}:
# 2026-10-06
Gamma design review moved to 10-09.


read_text_file {"path":"/home/me/notes/meetings/2026-10-13.md"}:
# 2026-10-13
Delta owner: Priya.
```

The agent made 1 call where it had made 3. [The walkthrough](/guide/walkthrough) runs this loop end to end on the official MCP filesystem server, with no key.

## The evidence

<p class="home-lead">Every number comes with the kind of evidence behind it, and the script that recomputes it.</p>

<div class="home-stats">
  <a href="research/claims">
    <strong>27.9%</strong>
    <span>fewer LLM turns for GLM-5.3 on 28 τ²-bench retail and airline tasks (19.1–35.9%), live, with no model of its own</span>
  </a>
  <a href="research/claims">
    <strong>86.4%</strong>
    <span>of retail's read-only ceiling taken across nine agents it never saw, 10.2 points more than a next-step speculator, in replay</span>
  </a>
  <a href="research/claims">
    <strong>10 sessions</strong>
    <span>of an agent's own give 96% (retail) and 93% (airline) of what all of them do, in replay</span>
  </a>
  <a href="research/claims">
    <strong>39 of 40</strong>
    <span>held-out solo telecom tasks passed by a compiled procedure with a model on its 4 hand-backs, at 0.48 LLM turns per ticket against 15.9</span>
  </a>
</div>

Read [why stretto](/guide/why) for where these come from and what they do not show, or [the paper](/research/paper) for the model behind them.

<!-- BRAND SLOT: the launch video (website/public/media/launch.mp4, poster media/launch-poster.png). Renders nothing until the file is there. -->
<BrandEmbed kind="launch" caption="stretto in two minutes." />
