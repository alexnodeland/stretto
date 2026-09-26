# Checking a write against the proposal, with no model

[The confirmation judge's second question](confirm-second-2026-09-24.md) found that the lapses that matter most are calls that differ from what the customer agreed to: a refund to a gift card after "my Mastercard ending in 2732", another order's laptop, flights other than the ones proposed. That page suggested a check that compares a call's arguments with the ids, items and amounts in the agent's last message, with no model. [`scripts/proposal_check.py`](../../scripts/proposal_check.py) is that check, tried offline.

## What it checks

Before each write it takes the agent's last message before the customer's last reply (the proposal) and the reply. For every value the write passes, it finds the record the value came from in an earlier tool result, among the records listed with it: an order among the user's orders, an item among an order's items or a product's variants, a card among the payment methods. A record is named when the text states its id (or the number in an id such as `credit_card_4196779`), or a field no other record of its list shares, such as an item's name or options, or a card's last four digits.

The write is flagged when the confirmation chose another record of that list and not the one the write passes. The reply chooses the records it names. Failing that, the proposal chooses a record when it names only that one of its list: a proposal that lists several offers a choice rather than making one. A record the write passes too (an exchange passes the old item and the new one) is never "another".

The first try flagged every value the proposal left unsaid. That flags half of all writes, since agents confirm "the original payment method" or "the water bottle" without an id, and a flight change passes every flight of the reservation (`--omissions`).

## Results

| Episodes | Writes in successful episodes flagged | In failed episodes |
|---|---|---|
| τ²-bench's 2025 baselines, retail (four agents) | 42 of 1,577 (2.7%) | 52 of 578 (9.0%) |
| τ²-bench's 2025 baselines, airline | 15 of 282 (5.3%) | 13 of 406 (3.2%) |
| Nine current agents, retail | 142 of 4,785 (3.0%) | 48 of 989 (4.9%) |
| Nine current agents, airline | 35 of 1,038 (3.4%) | 30 of 567 (5.3%) |

On the second question's blind labels that match these episodes (45 of 60), it flags 3 of the 6 calls that differ from what the customer agreed to, and 3 of 28 confirmed writes. It is blind by design to the other lapses: an address the customer typed that the agent never read back (0 of 5), and a change the customer added (0 of 3).

Some flags in successful episodes are real lapses τ²-bench does not score. In one, the agent summed up a return with "refund method: paypal (paypal_9497703)", the customer said yes, and the call refunded `credit_card_3124723`.

## What this means

- **Useful to log, not to enforce.** In retail it flags three times as many writes in failed episodes as in successful ones, and it needs no model and no key. But it would refuse about 3% of the writes of successful episodes, where [the guards](guards-2026-09-24.md) refuse about 1% in airline. In airline it does not separate them.
- **It catches half of the wrong-record lapses.** The rest name the record in words the check cannot tie to it, such as a cabin class or "the cheaper one".
- `stretto-proxy` logs it beside the confirmation judge (`proposal_check` in each judged write's entry), and refuses nothing. The proxy has no dataset to find closed choices in, so it checks only values with a digit, as ids carry them. On the 2025 runs above, that flags exactly the writes this page's closed-choice rule flagged.

## Reproduce

No keys:

```sh
R=../tau2-bench/data/tau2/results/final
python3 scripts/proposal_check.py $R/*_retail_default_gpt-4.1-2025-04-14_4trials.json \
  $R/*_airline_default_gpt-4.1-2025-04-14_4trials.json \
  --labels docs/results/confirm-second-2026-09-24-labels.json --json proposal-check.json
python3 scripts/proposal_check.py $(scripts/fetch-leaderboard.sh all | cut -d= -f2)
```

[proposal-check-2026-09-26.json](proposal-check-2026-09-26.json) holds the 2025 baselines' tallies and the label counts.
