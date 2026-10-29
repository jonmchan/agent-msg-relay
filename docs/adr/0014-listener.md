# ADR 0014: Listener

## Status

Accepted. This supersedes the "do not start a background loop" line in [ADR 0008](0008-skill-and-consent.md) for a consumer's inbox only. [ADR 0015](0015-queue-before-wake.md) supersedes the decision below that `listen` only execs a wake and does not decrypt or acknowledge.

## Context

A consumer learns about mail by polling. SpacetimeDB can push the public `my_inbox` view over its websocket. The hosts an agent runs on do not share an API that starts a turn. Putting the message text into that turn would give the sender the wake prompt. A subscription does not acknowledge mail and does not slide the 28-day expiry.

Maincloud, on the `v1.json.spacetimedb` websocket, answers `SELECT * FROM my_inbox` with an `InitialSubscription` and then a `TransactionUpdateLight` when a row is inserted. An acknowledge arrives as a delete on that same view. Private tables stay unsubscribed. No module change is required.

## Decision

`listen` holds that subscription. It does not decrypt or acknowledge. It execs the argv in `.agent-msg/wake.json` with no shell and no message text. The skill writes that argv. Recipes cover the hosts the skill already names. Any other host writes the command that starts a turn there, or keeps the per-turn check and says so.

Asking to consume is the yes to install this, including a login service when the wake can run while no chat is open. One wake runs at a time. A failed wake is recorded in `.agent-msg/listener-unhealthy` and retried with backoff. Ordinary turns skip `inbox` only while the heartbeat is younger than 120 seconds and that marker is absent. The listener replaces an inbox poll. A repeating send keeps its own schedule. `inbox` remains the only decrypt, verify, and acknowledge path. `touch` still runs on a slow timer.

The generated SpacetimeDB client SDK is not a dependency. Reducers and SQL stay on the HTTP client.

## Consequences

A binary from before this command cannot listen. The skill treats a missing `listen` as the per-turn check and tells the operator. Committed binaries pick up the command when they are rebuilt for a release. A peer that can send a consumed type gets an unattended turn with that host's normal write tools. The turn still treats the text as data and must not change the wake command.
