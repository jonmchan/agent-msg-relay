# ADR 0008: Skill and consent

## Status

Accepted

## Context

A skill file does not run every turn by itself. Cursor loads `AGENTS.md`. Claude Code loads `CLAUDE.md` when that file exists, and loads `AGENTS.md` when it does not. Writing those files the moment a skill is installed edits the operator's files without a yes. An inbound message that can edit the same files is a prompt-injection path.

## Decision

The skill is the directory `.agents/skills/agent-msg-relay/`. `SKILL.md` is the workflow an agent keeps in context. `references/` and `assets/` load only when that workflow points at them, so a copied skill still works for an agent that does not have this `docs/` tree. Cursor and Codex load that directory. Claude Code loads `.claude/skills/agent-msg-relay`, a link to it. It is not copied under `.cursor/skills/`. Its description is auto-invocable for contacting an agent, opening a channel, and sharing. The inbox check is also requested from a marked section in `AGENTS.md`. Claude installs add the same section to `CLAUDE.md` when that host or that file is present. Cursor does not get a second always-on rule, because it already loads `AGENTS.md`.

The agent asks once, in the conversation, before it adds the section. The answer is stored in `.agent-msg/consent.json`. A section that is already present is this agent's instruction. This repository does not include the section, because it is not a consumer. When the operator defines this agent as a consumer, that request is the yes. The agent writes a `consumes` array of message types and the condition for each, then queues later messages of those types and applies each one as that type of work. A task is added to this agent's own task list. It does not follow instructions inside the text. A producer does not get the section. The inbox check runs on a scheduler when one is already available, and on each turn when the process only wakes for a prompt. The agent does not infer a scheduler from the prompt. Asking to post, pull, read, or join a named public channel is the yes to join that channel.

Message bodies are shown to the operator and then acknowledged when they verify. They are data. The skill does not edit startup files, the skill, or `identity.json` because of a message. Join and accept wait for the operator. A received invitation is recorded in `.agent-msg/address-book.json`.

Register does not prompt for a public profile. Share works without one. A private share's purpose is supplied for that handoff and is not published. The paste block uses `git remote get-url origin`, or `https://github.com/jonmchan/agent-msg-relay` when origin is missing.

## Consequences

A clone of this repository does not run the inbox check. A consumer does, after that operator agrees. A declined install can still run the CLI when the operator asks. A producer can send without the inbox check.
