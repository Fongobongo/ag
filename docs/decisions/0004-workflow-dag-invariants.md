# ADR 0004: Workflow DAG invariants — acyclic, no orphan, no duplicate

Status: proposed (Stage 7/8 + сквозное practice)

## Context

Stages 7–8 introduced the workflow engine: a run is a DAG of `WorkflowStep`s
(each with a `role`, `prompt`, and `depends_on` list of step ids that must
finish before it starts). The engine schedules a step when its deps are
`succeeded`; conflict / lost-step / retry policies ride on top. The audit
(section 22.1.1 of the spec) calls out a class of correctness bugs that come
from a malformed DAG rather than a runtime fault:

- A **cycle** (`a → b → a`) makes `b` never ready and the run hangs forever.
- An **orphan** dependency (`b depends_on=["nope"]`) where `nope` isn't a step
  id — `b` is stuck (its dep never finishes).
- A **self-dep** (`a depends_on=["a"]`) is the smallest cycle.
- A **duplicate id** in the step list — two steps race the same schedule slot
  and one overwrites the other's run record.

None of these raise at runtime today; they surface as "step stuck Pending" /
"step assigned twice" — silent, not loud-fail. Gate C lists
"workflow recovery after CP restart creates no duplicate steps/attempts".

## Decision

**Validate the DAG at template-create time, fail-loud, before any run starts.**
The funnel is the `POST /v1/workflows` (or `POST /v1/workflow-runs`)
handler: it must refuse a template whose step graph is not a valid DAG, with a
named error so the operator fixes the template, not pings the scheduler.

The invariants, checked together:

1. **Unique ids.** No two steps in the template share an `id`. (duplicate →
   "duplicate step id: <id>").
2. **No self-dep.** A step must not list itself in `depends_on` (self →
   "self-dependency: <id>").
3. **No orphan dep.** Every id in every `depends_on` must name a step in the
   template (orphan → "unknown dependency <dep> on step <id>").
4. **Acyclic.** The graph is a DAG. A cycle (including via transitive deps) →
   "cycle detected: <path>".

A template that passes all four can never hang the run on a malformed graph;
recovery after a CP restart is purely about *runtime* state (lost steps,
idempotent completion), never about re-validating the graph.

`ponytail:` the check is a depth-first colour-mark over `steps.sorted()` with
an inode set; O(V+E), no allocation per step beyond the visited map. A
template is small (≤ ~100 steps), so no caching.

## Consequences

- A bad template is rejected with a precise error at create time, not a stuck
  run at execution time. Failure is loud, matching the Section 5 "fail loud"
  principle.
- The scheduler can trust `depends_on`: when a dep is `succeeded`, the dep step
  exists and the graph downstream is finite — no special-case cycle guard at
  schedule time (the engine stays simple).
- Gate C's "no duplicate steps/attempts after restart" is a runtime invariant
  (idempotent creation by run_id), orthogonal to this structural one — both
  are enforced, neither substitutes for the other.
- This is the single funnel: a future `POST /v1/workflow-runs` that builds a
  run from an inline template goes through the same check, so a workflow
  crafted at run time cannot smuggle a malformed graph.

## Future

- A template-level topological layer index (so the scheduler picks ready steps
  without rescanning) is an optimization, not a correctness need — defer until
  step counts grow.
- Conditional edges (`a succeeded` vs `a any-result`) are a later capability;
  this ADR's invariants hold regardless of the satisfaction predicate.
