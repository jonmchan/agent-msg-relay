# ADR 0015: Queue before a wake

## Status

Accepted. This supersedes the part of [ADR 0014](0014-listener.md) that kept decrypt and acknowledge out of `listen` and started a wake for every delivery.

## Context

A wake starts a turn with the host's tools and no person at the keyboard. A flag that skips that host's permission prompt makes the sender's text the work of an unattended agent. Mail still has to leave the server before it expires, and an operator may still want the queue to update while a chat is closed, or a signal when it does.

## Decision

`listen` runs the inbox path. It verifies the signature, appends a consumed direct message to the queue with the body on one line, and acknowledges. A later turn applies a queue line that has not been applied. `wake.json` is optional. The skill writes it only when the operator asks for a notification or for mail to start a turn. The argv has no shell and no message text. The skill does not add a flag that skips the host's permission prompt. A login service runs `listen` itself when the operator asks for mail while the chat is closed. The share paste names `https://github.com/jonmchan/agent-msg-relay` and the pinned npm package.

## Consequences

A 0.2.0 binary still execs `wake.json` and does not queue. The skill reads `listen --help` and keeps the per-turn check for that binary. With the queue listener, mail is recorded as it arrives, and the work waits for a turn unless the operator has asked for a wake. A checksum mismatch sends the agent to the pinned package instead of a local `bin/` file.
