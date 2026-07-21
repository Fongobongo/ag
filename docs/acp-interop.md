# ACP client interoperability (Poracode / Lightcode)

Stage 6 shipped `agentgrid-acp-agent`: a stdio ACP *agent* that bridges any
ACP-speaking client to the Agentgrid control plane. This note records what
works today and the known gaps, as a spike (no live Poracode/Lightcode runs
were available in this environment).

## What an ACP client gets for free

`agentgrid-acp-agent` speaks the standard ACP agent role over stdio, so any
compliant client can drive it without changes:

- `initialize` → returns protocol version `0.1`, empty capabilities.
- `session/new` → mints an Agentgrid session id, stores agent/model/cwd.
- `session/prompt` → creates an Agentgrid task, then streams task
  `status`/`tool_call`/`file_change`/`progress`/`result`/`error` events back as
  `session/update` until the task reaches a terminal state.
- `session/cancel` → cancels the backing task.
- `session/request_permission` → relayed from the control plane's approval
  queue; the client's `allow`/`deny` answer is posted back to the CP.

Environment: `AGENTGRID_SERVER` (required), `AGENTGRID_TOKEN` (optional).

## Known gaps / non-standard extensions

- **Extension methods are Agentgrid-specific.** Any `method` starting with `_`
  is routed to `handle_extension`; currently `_agentgrid/nodes`
  (`GET /v1/nodes`) and `_agentgrid/task_eligibility`
  (`GET /v1/tasks/{id}/eligibility`). Unknown `_` methods return a clean RPC
  error. A plain ACP client should ignore these.
- **No `session/load` / `session/resume` passthrough.** The gateway maps each
  `session/prompt` to a *new* Agentgrid task; it does not replay prior session
  history into the client. Multi-turn context stays inside the Agentgrid task's
  event log.
- **No `session/update` from client → control plane.** Client-sent
  `session/update` notifications are accepted (and ignored) today; they are not
  forwarded to the CP.
- **Capabilities negotiation is minimal.** `initialize` returns empty
  `capabilities`; clients that require specific capability flags may need a
  shim.

## Enforcement boundary (Stage 9.1)

The node daemon short-circuits `session/request_permission` through the
builtin `CommandPolicyProvider` **before** it creates an operator approval:

- `Allow` → the agent proceeds, no operator round-trip. A `permission_decision`
  status event is streamed so the operator still sees what was auto-permitted.
- `Deny` → the request is rejected outright.
- `Ask` → falls through to the durable approval flow
  (`POST /v1/tasks/{id}/approvals`, operator allow/deny).

**Boundary:** only Bash-style shell commands (`permission =
{tool:"Bash", input:"<cmd>"}`) are classified locally. Any other tool — or a
missing `input`, or a provider error — reaches the approval flow
(fail-closed to the operator). A **wrapper adapter** (an arbitrary binary
emitting JSON lines) without structured tool calls cannot be fully
intercepted by this layer: there is no `session/request_permission` to hook,
and its shell access goes directly through the node's sandbox. For a strict /
unattended profile, a wrapper adapter is therefore **not** a sufficient
guarantee — pair it with a sandbox backend policy (Stage 12). The ACP native
launcher is the forward path and is fully intercepted.

The autonomy level comes from `AGENTGRID_AUTONOMY` on the node (`l0`..`l4`,
default `l2`); the CP `POST /v1/policy/evaluate` mirrors the same matrix for
ad-hoc queries.

## Compatibility verdict

Poracode and Lightcode are ACP clients; both should connect to
`agentgrid-acp-agent` over stdio and run single-shot tasks. The gaps above are
ergonomic (resume, richer capability negotiation), not blocking. Verifying the
end-to-end handshake against live Poracode/Lightcode builds is left as a
follow-up (needs those binaries + credentials).
