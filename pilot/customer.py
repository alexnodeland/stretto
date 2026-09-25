"""The simulated customer.

τ²-bench's user-simulator prompt (its guidelines plus the task's scenario),
answered by GLM through Claude Code on Z.ai's coding endpoint, the route
Z.ai supports for its coding plan (see `glm-claude.sh`), or by a Claude
model (`cli="claude"`, see `claude-agent.sh`). Each reply is one headless
Claude Code call with no tools and our own system prompt, so it costs a
single short model request.
"""

import json
import os
import subprocess
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
# The wrapper each customer CLI runs through.
WRAPPERS = {"glm": HERE / "glm-claude.sh", "claude": HERE / "claude-agent.sh"}
MODEL = os.environ.get("PILOT_CUSTOMER_MODEL", "glm-5.3")


def reply(
    system_prompt: str, dialogue: list[tuple[str, str]], cli: str = "glm", model: str | None = None
) -> tuple[str, dict]:
    """The customer's next message after `dialogue`, a list of
    `(speaker, text)` with speaker `agent` or `customer`, and the usage it
    cost. `cli` names the wrapper (`glm` or `claude`) and `model` the model
    (default: PILOT_CUSTOMER_MODEL, else glm-5.3)."""
    transcript = "\n\n".join(
        f"{'Agent' if speaker == 'agent' else 'You'}: {text}"
        for speaker, text in dialogue
    )
    prompt = (
        "The conversation so far (you are the customer):\n\n"
        f"{transcript}\n\n"
        "Write your next message to the agent. Output only the message."
    )
    command = [
        str(WRAPPERS[cli]),
        "-p",
        "--bare",
        "--tools",
        "",
        "--strict-mcp-config",
        "--no-session-persistence",
        "--model",
        model or MODEL,
        "--system-prompt",
        system_prompt,
        "--output-format",
        "json",
        prompt,
    ]
    last = ""
    for attempt in range(3):
        done = subprocess.run(
            command,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=600,
            check=False,
        )
        try:
            out = json.loads(done.stdout)
        except json.JSONDecodeError:
            out = {"is_error": True, "result": done.stdout[-500:] or done.stderr[-500:]}
        if not out.get("is_error") and out.get("result"):
            return out["result"].strip(), out.get("usage", {})
        last = str(out.get("result"))
        time.sleep(5 * (attempt + 1))
    raise RuntimeError(f"customer simulator failed: {last}")
