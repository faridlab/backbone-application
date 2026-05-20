# Clean Code — naming, size, errors, comments

Rules below are MUST/SHOULD/NEVER. When in conflict with [`solid.md`](./solid.md) or [`layers.md`](./layers.md), those win.

## Naming (Rust conventions, restated because LLMs slip)

- **MUST** use `snake_case` for functions, methods, variables, modules, file names.
- **MUST** use `PascalCase` (UpperCamelCase) for types, traits, enum variants.
- **MUST** use `SCREAMING_SNAKE_CASE` for `const` and `static`.
- **MUST** use lifetime names like `'a`, `'src`, `'pool` — single short word.
- **SHOULD** match the business term. `Account`, not `AccountObj`. `Invoice`, not `InvEntity`.
- **NEVER** use Hungarian notation (`s_name`, `i_count`, `pPool`).
- **NEVER** use abbreviations the business doesn't use (`acct`, `cust`, `pmt` — write `account`, `customer`, `payment`).
- Module name and type name should rhyme: `account_handler.rs` contains `AccountHandler`, `invoice_service.rs` contains `InvoiceService`.

## File size (granularity)

- **MUST** split a file when it exceeds **500 lines**. The split should follow a real seam — by responsibility, sub-module, or entity — not by line count alone.
- **SHOULD** keep new files under ~300 lines as the target.
- **SHOULD** mirror the existing folder structure when splitting. If `account_handler.rs` grows past 500 lines and it handles both read and write endpoints, split into `account_handler/read.rs` and `account_handler/write.rs` under an `account_handler/mod.rs`.
- **NEVER** split arbitrarily to satisfy the line count (e.g. `account_handler_part2.rs`). If there's no real seam, the file is doing too much — refactor responsibilities first.
- **NEVER** split generated files by hand. Fix the generator (or the schema YAML upstream) and regenerate.

## Function size

- **SHOULD** keep functions under ~50 lines. Longer needs a reason (e.g. a state-machine `match` that genuinely has 20 arms).
- **MUST** split a function once it has more than one clear "and then" step — extract each step into a named helper.
- **MUST NOT** add a function whose body is one line just to "make it composable" if it has one caller. Inline.

## Error handling

- **MUST** use `thiserror` for typed error boundaries between layers (repository errors, service errors, handler errors).
- **MUST** use `anyhow` only inside [`src/main.rs`](../../src/main.rs) (bootstrap may bubble anything) and in tests.
- **NEVER** `unwrap()` or `expect()` in production code paths. Allowed only in:
  - tests,
  - `main.rs` bootstrap before the runtime is up,
  - cases where the framework's type system already guarantees the invariant (rare; document with a `// Safe: ...` if you do it).
- **MUST** propagate with `?` rather than nested `match` when the error type already converts via `From`.
- **MUST NOT** convert a typed error into `String` and lose the type. The error envelope in [`src/shared/`](../../src/shared/) carries the kind through to the response.
- **MUST NOT** silently swallow an error (`let _ = something_that_returns_result();`). Either handle it, log it with context, or propagate.

## Logging

- **MUST** use `tracing` (`info!`, `warn!`, `error!`, `debug!`), not `println!` or `eprintln!`.
- **MUST** attach structured fields (`tracing::info!(record_id = %id, "record created")`), not interpolate values into the message string.
- **SHOULD** log at the layer where the decision happens. Don't log the same event twice in handler and service.
- **NEVER** log secrets (JWT, API keys, raw password bodies, full email payloads).

## Comments policy

- **Default: write no comments.** Well-named identifiers explain the *what*.
- **Comments MUST explain *why*, never *what*.** "Increments counter" — delete. "Counter resets at midnight UTC because the billing window aligns there" — keep.
- **NEVER** write task-tracking comments: `// added for ticket FOO-123`, `// per @someone's request`, `// see PR #456`. PRs and git log own that.
- **NEVER** write "removed X" or "previously did Y" trailing comments. Just remove the code.
- **NEVER** leave commented-out code. Delete it; git remembers.
- Doc comments (`///`) on public types/functions: **SHOULD** for anything in `src/exports/` and any `pub` API consumed by other modules. Short and to-the-point — one paragraph plus an example if non-obvious.

## Simplicity (MUST form of the behavioural rule)

- **MUST** write the minimum code that satisfies the request. Nothing speculative.
- **MUST NOT** add features beyond what was asked. No "while I'm here…".
- **MUST NOT** introduce an abstraction for a single caller. Inline.
- **MUST NOT** add "flexibility" or "configurability" that wasn't requested. YAGNI.
- **MUST NOT** add error handling for scenarios the type system already rules out.
- **SHOULD** ask: "Would a senior engineer say this is overcomplicated?" If yes, simplify before submitting.

## Async / concurrency

- **MUST** use async equivalents inside Tokio tasks. **NEVER** `std::fs::read`, `std::thread::sleep`, `reqwest::blocking`, or any blocking call.
- **MUST** use `tokio::sync::Mutex`/`RwLock` when holding across `.await`. Plain `std::sync::Mutex` blocks the runtime if held across an await point.
- **SHOULD** prefer message passing (`tokio::sync::mpsc`) over shared mutable state when possible.

## Tests

- **MUST** add an integration test for any new HTTP/gRPC endpoint. Place it under `tests/` following the existing layout.
- **MUST** keep test names descriptive: `fn rejects_request_when_field_is_empty()`, not `fn test_handler_1()`.
- **MUST NOT** assert against full response bodies when only one field matters. Asserts should pinpoint the contract, not the encoding.
- **SHOULD** prefer running against a real database in integration tests (we have the dev stack for that); mocks lie about migrations.

## Imports

- **MUST** group imports `std → external crates → crate-local`, separated by blank lines (matches `rustfmt` default).
- **MUST NOT** glob-import entity modules (`use crate::domain::entity::*;`). Be specific.
- **SHOULD** prefer `use module::Type` once at the top over fully-qualifying in code.

## Checks before submitting

```bash
metaphor lint check     # clippy + fmt + audit
metaphor dev test       # unit + integration
wc -l <changed file>    # any new file > 500 lines? split.
```
