# Insight Evaluation

Status: local-v1 alpha baseline rule.

## Versioned Launch Rule

- Config version: `local-v1-alpha-2026-06-20-windowed-pooled-rates`
- Minimum comparable baseline days: 3
- Relative change threshold: 25%
- Current comparison window: 2 consecutive local days; both must cross the
  threshold in the same direction
- Maximum comparable-baseline relative median absolute deviation: 50%
- Default calendar policy: pool all prior recorded days containing the signal
- Coverage policy: compare cumulative switching as switches per recorded hour,
  and active durations as a share of recorded periods

This rule is intentionally conservative and deterministic. Cards compare a
multi-day current window with an earlier multi-day baseline; a single day can
never produce one. It is not tuned to maximize the number of cards. It should remain quiet when
data is sparse, zero-centered, unstable, missing, or moved for only one isolated
day.

The app surfaces only the latest complete comparison window per dimension.
Older changed windows remain visible in the raw daily history, not as current
rhythm-change cards.

## Synthetic False-Nudge Budget

The current local synthetic budget is zero false nudges across these quiet
fixtures:

- stable routine with ordinary noise;
- one isolated spike;
- zero-heavy count baseline;
- unstable baseline.

The same test requires a sustained synthetic shift to produce inspectable
descriptions for at least browser tabs and energy, with neutral language.

A separate schedule-shift fixture intentionally produces a neutral description
from a complete changed two-day window. It cites the exact prior dates so the user can decide whether
workdays, rest days, travel, or another context explains the change.
This is description, not a claim that the schedule caused it.

## Calendar Policy

Weekday/weekend differences are plausible context, but a fixed Saturday/Sunday
split assumes a conventional work schedule and applies the same assumption to
every person and signal. Some research separates weekdays and weekends, but
explicitly uses that split as a proxy when actual work schedules are unknown.
Observed screen-use and mood differences also vary by population and context.

The v1 rule therefore pools prior days by default. Raw history is never grouped
or hidden. A future advanced comparison calendar may let the user define work
and non-work days; it must affect descriptions only, remain optional, and name
the selected baseline in every evidence bundle. Adaptive or partial-pooling
models remain research work, not launch behavior.

Daily totals remain visible in History. Baseline descriptions do not compare a
partial current-day total with completed prior-day totals: tab switching is
normalized per recorded hour, and browser/computer active time is normalized
as the active share of recorded 15-minute periods. Every card still reports its
current-window period coverage.

Research annotations:

- Saeb et al. separated workdays and non-workdays using Monday-Friday and
  Saturday-Sunday because participant schedules were unavailable:
  https://pmc.ncbi.nlm.nih.gov/articles/PMC5361882/
- Trace studies have observed different smartphone use on weekdays and
  weekends, without establishing that the distinction applies equally to each
  person or signal:
  https://pmc.ncbi.nlm.nih.gov/articles/PMC8856513/
- Daily mood research has found weekly cycles alongside substantial
  within-person variation:
  https://pmc.ncbi.nlm.nih.gov/articles/PMC2414486/
- Personal-informatics research recommends balancing automation with user
  control and treating tracking as part of lived context:
  https://www.cs.cmu.edu/~jhm/Readings/2010-ianli-chi-stage-based-model.pdf
  and https://doi.org/10.1145/2556288.2557039
- Ethical digital-phenotyping guidance emphasizes transparency, autonomy, bias,
  and accountability:
  https://pmc.ncbi.nlm.nih.gov/articles/PMC8367187/

Run the deterministic check with:

```bash
cargo test -p mindcanary-analytics launch_rule_meets_the_synthetic_false_nudge_budget
```

## What This Does Not Prove

This is not clinical validation, treatment evidence, or proof that the language
will feel right to real users. It is only a local regression gate for the
deterministic launch rule. Closed beta still needs real histories,
comprehension checks, and tracking-fatigue measurement.
