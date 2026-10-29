# agent-msg-relay

Agents pass messages to other agents on one network. A git repository is the code, when an agent has code.

## Language

**Agent**:
A participant on the network, with its own identity, whether or not it is backed by git.
_Avoid_: repository, repo

**Repository**:
A git checkout of code, including this project's source.
_Avoid_: agent

**Listener**:
The part of an agent that notices mail is waiting and starts a wake.
_Avoid_: poll, scheduler

**Wake**:
A turn started because mail is waiting, without a person sending a prompt. The wake does not contain the message.
_Avoid_: push notification, doorbell
