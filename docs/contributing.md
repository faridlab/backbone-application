# Contribution Guide

> **Reader:** contributor — you're proposing a change to this repo. **Mode:** how-to. Everything you
> need to open a PR that lands on the first try.

## Dev setup

```bash
git clone <this-repo> backbone-app && cd backbone-app
docker compose up -d          # postgres + redis + minio
cargo run                     # confirm it boots and /health responds
curl http://localhost:8080/health
```

Prerequisites and the local-dev options (host cargo vs. production-shape compose) are in the
[developer guide](./developer-guide.md#1-install). If you're developing the framework crates
alongside the service, the `[patch]` block in [`Cargo.toml`](../Cargo.toml) redirects the
`backbone-*` deps to `../backbone-framework/`.

## Branch & commit conventions

- **Branch off `main`.** Never commit directly to `main`.
- **Conventional Commits.** `type(scope): summary` — e.g. `feat: wire accounting module`,
  `fix(config): honor LOG_FORMAT override`, `docs: add maintainer guide`, `chore: bump crates to v2.1.0`.
  Match the existing history (`git log --oneline` shows `deploy:`, `chore:`, `feat:` in use).
- **One-line, imperative summary that states *why*.** Not "update main.rs" — say what changed and
  the reason. Group related files; keep large files in their own commit.
- **NEVER add a signature.** No `Co-Authored-By`, no "Generated with", no Claude/AI attribution in
  commit messages. This is a hard workspace rule ([root `CLAUDE.md`](../../CLAUDE.md)).

## Scope discipline

This repo is a **composition root** — the bar for adding code here is high.

- **Business logic does not belong here.** If your change adds an entity, an invariant, a domain
  rule, or a feature endpoint, it belongs in the owning `module` project upstream — open the PR
  there and bump the tag here. See the [add-a-feature decision tree](./maintainer-guide.md#add-a-feature-the-decision-tree).
- **Keep each PR within a single bounded context.** Cross-context changes need an explicit note.
- **Surgical changes only.** Every changed line traces to the request. Don't "improve" adjacent code,
  reformat untouched files, or refactor what isn't broken. Match the existing style.
- **Regen safety:** if you add a hand-written file inside a generator-owned tree, add its path to
  `metaphor.codegen.yaml` → `user_owned:` in the *same* PR, or it'll be silently deleted on the next
  regen.

## Run the checks (before you push)

```bash
# workspace (preferred)
metaphor lint check                     # clippy + fmt + audit
metaphor dev test                       # unit + integration
metaphor dev test --integration-only    # end-to-end

# standalone fallback
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI uses `--affected --base=main` to rebuild/retest only what changed — keep your diff scoped so the
affected set stays small.

## PR checklist

Before you open the PR, confirm:

- [ ] Change is composition/wiring, not business logic (or it's in the right upstream repo).
- [ ] `metaphor lint check` passes (clippy clean, formatted, audit clean).
- [ ] `metaphor dev test` passes — new behavior has a test; a bug fix has a reproducing test first.
- [ ] No file exceeds 500 lines (split along a real seam if it does).
- [ ] No cross-layer imports (`domain/` free of `sqlx`/`axum`/`tonic`/`reqwest`).
- [ ] Any hand-written file in a generator tree is listed under `user_owned:`.
- [ ] No secrets committed; config reads from env/overlays.
- [ ] Commit messages are Conventional Commits with **no signature**.
- [ ] `git status` after any regen shows only intended `M`/`D`.

## Review expectations

- A reviewer will check the layer boundaries and the scope discipline above first — a leak or an
  out-of-scope "improvement" is the most common bounce.
- Claims of "done" must be verified, not intended. If you haven't run it, say so and say what you'd
  run — see [ai-guidelines §8](./architecture/ai-guidelines.md).
- Architectural decisions get an [ADR](./adr/README.md); don't bury a decision in a code comment.

## Reporting a bug / proposing a decision

- **Bug:** include the failing command, the observed vs. expected output, and the environment
  (`APP_ENV`, compose path). A reproducing test in the PR is worth more than prose.
- **Decision:** open an ADR (copy [`adr/0001`](./adr/0001-subprocess-dispatched-plugins.md) as a
  template) with context, decision, status, consequences. ADRs are immutable once accepted —
  supersede, don't edit.
