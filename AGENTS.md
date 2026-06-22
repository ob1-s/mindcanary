# MindCanary Development Guidelines for AI Agent Staff

- Non-negotiable v1: private rhythms journal, optional check-ins,
  aggregate-only connectors, descriptive baselines. No diagnosis, prediction,
  emergency framing, hidden profiling, cloud dependency, or AI-controlled
  scoring.
- Keep **every** connector and higher-risk permission optional. Describe patterns;
  let users assign meaning. No streaks, scores, shame colors, rankings, guilt
  copy, nag notifications, or red-alert outlier states for ordinary changes.
- Treat pre-baseline value as product-critical: the app is first a private
  logbook with context, then a personal rhythm explainer once enough history
  exists.
- Treat baseline grouping as interpretation. Use pooled personal history by
  default; never silently assume a Monday-Friday workweek. Any future
  schedule-aware comparison must be user-selected and named in its evidence.
- Preserve annotation as user-owned meaning. Day/time-window notes can provide
  context for users or professionals, but MindCanary should not decide what a
  pattern means.
- Prefer direct implementation over architecture growth. Keep changes scoped to
  the active launch blocker and avoid broad fallback layers unless a real user
  failure justifies them.
- Do not over-defend the code with redundant guards, duplicate smoke tests, or
  speculative abstractions. Add the smallest durable check that proves the
  behavior changed.
- Preserve privacy boundaries: no URLs, titles, page text, search terms,
  message content, or raw browsing history in storage, logs, protocol payloads,
  docs, or tests.
- Treat the default local profile as live data. Before migrations, destructive
  checks, or clean-state onboarding tests, create a local export and use an
  isolated development profile.
- Use existing repo patterns. Rust owns protocol truth; regenerate TypeScript
  with `pnpm protocol:generate` after protocol changes.
- Validate focused paths after edits. Prefer targeted tests/builds that match
  the touched surface; run broader checks only when shared contracts changed.
- For meaningful desktop UI changes, run Tauri with an isolated development
  profile and visually inspect native screenshots at desktop and narrow widths.
- Keep Chrome optional. The desktop app must remain useful with check-ins only.
- Be autonomous with local work. Do not ask for permission for ordinary edits or
  commands when the environment allows them.
- No-immediatism is non-negotiable: insight cards compare windows, never a
  single day, no matter how strong the evidence. The Today view shows only
  input - check-ins, annotations, and same-day facts with no baseline
  comparison and no suggested action. Sustained-window descriptions belong in
  History only.
- Distinguish noise from tone inside "fake-actionable": a same-day fact with no
  comparison and no prescription is not immediatism and may stay in Today.
  Hiding data does not fix patronizing copy, and removing comparison does not
  fix a one-day spike - both controls are required; neither substitutes for the
  other.
- The pre-baseline period needs an honest, non-comparative progress affordance
  (for example, "11 of 14 days logged"). It must not reset, punish gaps, or
  imply daily performance - it counts toward readiness, it does not score a
  day.
- Waiting and empty states name the wait as a feature, not an error or a bug.
  Reference copy: "the canary is listening."
- When a task would benefit from long-running execution, self-manage the `/goal` interface until all deliverable criteria are met.
