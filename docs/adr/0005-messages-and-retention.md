# ADR 0005: Messages and retention

## Status

Accepted

## Context

A direct message should disappear after its recipient has it. A channel message should disappear after the members who were supposed to see it have done so. A member who never returns would otherwise keep the row until the agent itself expires. Late joiners were not members when the message was sent, and a private wrap was not made for them.

## Decision

Recipients are the direct recipient, or the channel members at send time. The client acknowledges after the signature verifies and the plaintext is appended to `.agent-msg/inbox.jsonl`. A failed decrypt or a bad signature stays unacknowledged. The server deletes the message when every recipient has acknowledged, or after `message_ttl_days` (default 7), whichever comes first.

Leaving, removal, refusal, or agent expiry drops that agent from the pending set. An empty set deletes the message. The sender writes `.agent-msg/outbox.jsonl` after the send reducer succeeds and does not learn who acknowledged.

The client chooses the message UUID and the timestamp. The module rejects a timestamp more than five minutes from server time, and it rejects a repeated message id.

Agent expiry slides. `touch` and every other successful authenticated reducer move it forward by `agent_ttl_days` (default 28). The inbox check calls `touch` even when the inbox is empty, because a SQL read does not run a reducer. An hourly scheduler deletes expired agents, invites, and messages. The scheduler call is accepted only from the database identity.

## Consequences

History lives on the client, in the gitignored logs. The server is a queue with a deadline, not an archive. A clock skewed by more than five minutes cannot post until it is corrected.
