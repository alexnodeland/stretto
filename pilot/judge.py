"""A judge for τ²-bench's natural-language assertions that a key here reaches.

τ²-bench judges a task's natural-language assertions with an LLM through
litellm (`tau2.config.DEFAULT_LLM_NL_ASSERTIONS`, GPT-4.1 by default), which
needs that provider's key. `use(spec)` answers the same prompt with another
model instead: `claude:MODEL` runs a Claude model through `claude-agent.sh`,
on Claude Code's own endpoint and credentials, with no tools. The judge's
prompt, the assertions and the rule (every assertion met, or the reward is 0)
stay τ²-bench's.
"""

import json
import re
import subprocess
from pathlib import Path
from types import SimpleNamespace

import tau2.evaluator.evaluator_nl_assertions as nl

WRAPPER = Path(__file__).resolve().parent / "claude-agent.sh"


def parse(spec: str) -> str:
    """The Claude model a `claude:MODEL` spec names."""
    kind, _, model = spec.partition(":")
    if kind != "claude" or not model:
        raise SystemExit(f"--judge {spec}: give claude:MODEL, such as claude:claude-haiku-4-5")
    return model


def answer(text: str) -> str:
    """The JSON object in a model's reply, without a code fence around it."""
    fenced = re.search(r"```(?:json)?\s*(\{.*\})\s*```", text, re.S)
    if fenced:
        return fenced.group(1)
    start, end = text.find("{"), text.rfind("}")
    return text[start : end + 1] if start >= 0 and end > start else text


def claude(model: str):
    """τ²-bench's `generate`, answered by `model` through claude-agent.sh."""

    def generate(*, messages, **_):
        prompt = "\n\n".join(f"{m.role.upper()}:\n{m.content}" for m in messages)
        prompt += "\n\nReply with the JSON object alone."
        reply = ""
        for _ in range(2):
            done = subprocess.run(
                [str(WRAPPER), "-p", "--model", model, "--output-format", "text", "--max-turns", "1"],
                input=prompt, capture_output=True, text=True, timeout=600,
            )
            reply = answer(done.stdout)
            try:
                json.loads(reply)
                return SimpleNamespace(content=reply)
            except json.JSONDecodeError:
                continue
        raise RuntimeError(f"the judge ({model}) did not answer in JSON: {done.stdout[:300]!r}")

    return generate


def use(spec: str) -> str:
    """Judge natural-language assertions with the model `spec` names; its name."""
    model = parse(spec)
    nl.generate = claude(model)
    return model
