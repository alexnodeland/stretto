"""Run a list of check_flow.py replays, resuming after an interruption.

A study is a JSON file: settings shared by every run, and the runs, each
with its own settings and an `out` directory. A run whose `out` already
holds a `check.json` is skipped, so an interrupted study picks up where it
stopped. Each run's settings are check_flow.py's options, named as its
flags without the dashes (`flow_decider`, `trials`, ...); a list becomes
several values, and `true` a bare flag.

    {
      "common": {"domain": "retail", "oracle_cache": "../.oracle-cache",
                 "flow_oracle": "replay", "trials": [0, 1, 2, 3],
                 "in_process": true, "jobs": 4},
      "runs": [
        {"out": "runs/study/habit", "flow": "flows/retail-5.flow.json", "flow_decider": "habit",
         "results": "../.data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json"},
        {"out": "runs/study/arbiter", "flow": "flows/retail-5.flow.json", "flow_decider": "arbiter",
         "results": "../.data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json"}
      ]
    }

    python replay_study.py study.json
"""

import json
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent


def flags(settings: dict) -> list[str]:
    out = []
    for name, value in settings.items():
        flag = "--" + name.replace("_", "-")
        if value is True:
            out.append(flag)
        elif value is False or value is None:
            continue
        elif isinstance(value, list):
            out += [flag, *map(str, value)]
        else:
            out += [flag, str(value)]
    return out


def main() -> None:
    study = json.loads(Path(sys.argv[1]).read_text())
    common = study.get("common", {})
    for run in study["runs"]:
        settings = {**common, **run}
        out = Path(settings["out"])
        if (out / "check.json").exists():
            print(f"skip {out}: done", flush=True)
            continue
        out.parent.mkdir(parents=True, exist_ok=True)
        start = time.time()
        with open(out.with_suffix(".log"), "w") as log:
            done = subprocess.run([sys.executable, str(HERE / "check_flow.py"), *flags(settings)],
                                  cwd=HERE, stdout=log, stderr=subprocess.STDOUT, check=False)
        if done.returncode or not (out / "check.json").exists():
            print(f"FAILED {out} (exit {done.returncode}); see {out.with_suffix('.log')}", flush=True)
            continue
        total = json.loads((out / "check.json").read_text())["total"]
        print(f"{out}: {total['turns_saved']} of {total['turns']} turns saved, {total['detours']} detours, "
              f"{time.time() - start:.0f} s", flush=True)


if __name__ == "__main__":
    main()
