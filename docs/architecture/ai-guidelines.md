# AI Guidelines — must-follow rules for Claude on this codebase

These rules bias toward caution over speed. For trivial tasks, use judgment. For anything non-trivial, follow them in order.

## 1. Think before coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

- **MUST** state your assumptions explicitly before you write code. If you're guessing, say so.
- **MUST** present alternatives when multiple interpretations of the request exist. Don't pick silently.
- **MUST** push back when a simpler approach exists. Say so plainly.
- **MUST** stop and ask when something is unclear. Name what's confusing — don't soldier on with a guess.
- **SHOULD** read the relevant files first (use Read / Grep), not invent based on plausible-sounding names.

## 2. Simplicity first

**Minimum code that solves the problem. Nothing speculative.** Cross-references the MUST form in [`clean-code.md`](./clean-code.md#simplicity-must-form-of-the-behavioural-rule).

- **MUST** write the minimum code that satisfies the request.
- **NEVER** add features the user didn't ask for.
- **NEVER** add abstractions for single-use code. Inline it.
- **NEVER** add "flexibility" or "configurability" that wasn't requested.
- **NEVER** write error handling for impossible scenarios (e.g. validating a `u32` is non-negative).
- **MUST** ask before submitting: *"Would a senior engineer say this is overcomplicated?"* If yes, simplify.

## 3. Surgical changes

**Touch only what you must. Clean up only your own mess.**

- **MUST** make every changed line trace directly to the user's request.
- **NEVER** "improve" adjacent code, comments, or formatting that wasn't part of the ask.
- **NEVER** refactor things that aren't broken.
- **MUST** match the existing style, even if you'd do it differently.
- **MUST** mention unrelated dead code if you spot it — but **NEVER** delete it without being asked.
- **MUST** remove imports, variables, functions that **your** changes made unused.
- **NEVER** remove pre-existing dead code as a side effect.

## 4. Goal-driven execution

**Define success criteria. Loop until verified.**

- **MUST** convert the task into a verifiable goal:
  - "Add validation" → "Write tests for invalid inputs, then make them pass."
  - "Fix the bug" → "Write a test that reproduces it, then make it pass."
  - "Refactor X" → "Ensure tests pass before and after."
- **MUST** state a brief plan for multi-step work:
  ```
  1. [Step] → verify: [check]
  2. [Step] → verify: [check]
  3. [Step] → verify: [check]
  ```
- **MUST** verify before claiming done. Run the test, hit the endpoint, read the file you just wrote.
- **NEVER** declare a task complete based on what you intended to do. Verify what actually happened.

## 5. Architecture boundaries

Cross-references [`layers.md`](./layers.md) and [`overview.md`](./overview.md).

- **MUST** respect the layer dependency rule. Inner layers (`domain/`) MUST NOT import outer-layer crates (`sqlx`, `axum`, `tonic`, `reqwest`).
- **MUST** use the framework primitives (`BackboneCrudHandler`, `GenericCrudService`, `GenericCrudRepository`) before hand-rolling.
- **NEVER** edit anything under a workspace's `modules/*` tree — those are read-only clones, wiped by the next `metaphor sync`. Fix upstream, bump `ref:` in the workspace `metaphor.yaml`, `metaphor sync --update`.
- **NEVER** hand-edit `metaphor.lock`. Regenerate via `metaphor sync --update`.
- **NEVER** copy files between `apps/*`. If two apps need shared code, it belongs in an upstream module.
- **NEVER** put business logic in [`src/main.rs`](../../src/main.rs). It belongs in a module.
- **NEVER** hand-write SQL migrations when the module's schema YAML can regenerate them.
- **MUST** list any new hand-written file inside a generator-owned tree (e.g. a custom service under `src/application/service/`, a custom adapter under `src/infrastructure/integration/`, a manual migration under `migrations/`) in [`metaphor.codegen.yaml`](../../metaphor.codegen.yaml) → `user_owned:`. Files not listed there and not wrapped in `// <<< CUSTOM ... // END CUSTOM` markers are wiped on the next `metaphor schema generate --force`.
- **MUST** wrap any hand edit inside a generator-emitted file in `// <<< CUSTOM ... // END CUSTOM` markers. The marker block must contain complete syntactic units (full `pub mod foo;` / `pub use foo::*;` statements, not field fragments).
- **MUST** run `cargo check` immediately after any regen and review `git status` for unexpected deletions before doing anything else.

## 6. Bounded-context boundaries

Cross-references [`ddd.md`](./ddd.md).

- **MUST** push feature work into the **owning upstream module**, not this service's `domain/`. Bumps come back here via the workspace `metaphor.yaml`.
- **NEVER** duplicate an entity across bounded contexts. One owns it; others translate at the boundary (`src/integration/context_map.rs` if you have one).
- **MUST** keep each PR within a single bounded context. Cross-context PRs need an explicit note.

## 7. File granularity

Cross-references [`clean-code.md`](./clean-code.md#file-size-granularity).

- **MUST** split any file that grows beyond **500 lines** along a real seam (responsibility / sub-module / entity). Don't split arbitrarily.
- **SHOULD** keep new files under ~300 lines.
- **NEVER** create `foo_part2.rs` style splits. If there's no real seam, the file is doing too much — refactor responsibilities, then split.
- **NEVER** split generated files by hand. Fix the generator or the schema YAML upstream.

## 8. Verification before "done"

- **MUST** run the right check for the change you made:
  - Code change: `metaphor lint check && metaphor dev test`
  - HTTP endpoint added: hit it with `curl` against the dev stack and confirm the response shape.
  - Migration / schema change: `metaphor migration run-all` against the dev DB.
  - Frontend / UI change (n/a here, but for completeness): exercise it in a browser.
- **MUST** read the diff before announcing completion. Look for: cross-layer imports, missed orphans, accidental edits to `modules/*`, files > 500 lines.
- **NEVER** say "should work" without proof. Either run it or say "I haven't verified — here's what I'd run."

## Why these guidelines work

- Fewer unnecessary changes in diffs.
- Fewer rewrites caused by overcomplication.
- Clarifying questions arrive **before** implementation, not after a mistake.
- The codebase keeps the shape it was designed to have (layers, contexts, file size) over many AI-assisted edits.

If you're about to break one of these rules and you think it's justified, **say so out loud first** and let the user decide.
