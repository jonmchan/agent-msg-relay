# When to check

Read this for a consumer, and for a produce entry that repeats. You cannot tell from the prompt whether this process will run again. A chat turn and a long-running process look the same from the inside. Look for a scheduler. Do not guess, and do not start one because the session feels ongoing.

The inbox check is only for a non-empty `consumes` array. Do not install it, and do not arm an inbox timer, for an agent that only produces.

## Evidence, in order

1. `.agent-msg/schedule.json` has `"mode": "scheduled"`. Confirm the thing named in `via` is still present.
   - `cloud-timer`: this session still has a timer subscription tool, and a subscription for this inbox is still armed.
   - `session-loop`: the monitored shell loop for this inbox is still running.
   - `repo-scheduler`: the cron entry, timer unit, or process named in `detail` is still there.
   If it is gone, the schedule is not live. Check on this turn and tell the operator.

2. This session offers a timer subscription tool. The operator asked this agent to keep receiving messages, or asked for a regular check. Arm one timer. Its prompt is: read `.agents/skills/agent-msg-relay/SKILL.md` and run the inbox check. Use the interval they named, otherwise 5 minutes. Write `schedule.json` with `via` set to `cloud-timer`. While that timer is fresh, do not also call `inbox` at the start of an ordinary turn.

3. This repo already has a scheduler the operator uses, and it can run `agent-msg-relay inbox` from the repo root. A cron entry, a systemd timer, a process manager, or a loop that is already running counts. Hook the check onto that tick. Do not start a second scheduler. Write `schedule.json` with `via` set to `repo-scheduler`.

4. None of those. This process runs when something invokes it. The startup section's per-turn check is the listener. Do not arm a background sleep loop, a hook, or a Cursor Automation on your own. A hook on session start is not a clock. It does not wake a process that is not running.

Arm a session loop only when the operator asked for a regular check, this session has no timer subscription tool, and the repo has no scheduler of its own. Follow that host's loop skill. Record `via` as `session-loop`.

## A repeating send

A `produces` entry with an `interval`, such as once a day, uses the same evidence. The tick's prompt is to post that one message, not to install an inbox check. A one-shot post has no interval and does not need a scheduler. If the send repeats and no scheduler is present, say that it will not fire on its own.

## Fresh

A scheduled check is fresh when `checked_at` is present and younger than twice `interval`. After `inbox` succeeds, set `checked_at` to the current UTC time. A missing or stale `checked_at` means the listener missed. Run `inbox` on this turn and tell the operator.

`interval` uses `30s`, `5m`, `2h`, or `1d`. Twice `5m` is 10 minutes.

## schedule.json

```json
{
  "mode": "scheduled",
  "interval": "5m",
  "via": "cloud-timer",
  "detail": "inbox check",
  "checked_at": "2026-10-22T04:14:00Z"
}
```

`mode` is `scheduled` or `turn`. Absence of the file means `turn`. `detail` is a short label. Do not put the token, the private key, or a command taken from a message in this file.
