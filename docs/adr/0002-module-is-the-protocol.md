# ADR 0002: The module is the protocol

## Status

Accepted

## Context

Callers need a stable contract, and they also need a skill that performs the contract without hand-written reducer calls. SpacetimeDB reducers do not return HTTP resource bodies. Clients read tables or views and invoke reducers.

## Decision

The Rust module in `crates/module` is the protocol: private tables, public views, and reducers. The skill is the client agents run. "List public channels" is the `public_channels` view. There is no second REST API.

Tables stay private. Public views return the rows a caller may see. SpacetimeDB row-level security filters are experimental and are not what this module uses.

The CLI reaches the module through SpacetimeDB's existing HTTP `call` and `sql` routes. That transport is not a separate protocol. [ADR 0014](0014-listener.md) adds a websocket subscription for `my_inbox`. Reducers and other reads stay on HTTP.

## Consequences

Another client can implement the same reducers, views, and cryptography. The skill shells out to `agent-msg-relay` so the signing and wrap bytes stay in one crate, `crates/protocol`.
