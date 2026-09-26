"""check_flow.py, with each replayed episode's calls tagged by author, for
relearning from replayed served sessions (scripts/relearn_round.py).

Writes tags.json in each episode directory: [["agent"|"flow", tool, args], ...]
in the order the server recorded them in trajectory.jsonl."""
import json, sys
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parent))
import check_flow

orig_walk = check_flow.walk


async def walk(messages, episode, call, task_id):
    log = []

    async def tagged(name, arguments):
        text = await call(name, arguments)
        log.append(["agent", name, arguments])
        for tool, args in check_flow.flow_calls(text):
            log.append(["flow", tool, json.loads(args)])
        return text

    row = await orig_walk(messages, episode, tagged, task_id)
    (episode / "tags.json").write_text(json.dumps(log))
    return row


check_flow.walk = walk
if __name__ == "__main__":
    check_flow.main()
