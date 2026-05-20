# SOLID — in this codebase

Each principle with a "good in this codebase" and a "violation looks like" pair. When in doubt, prefer the framework's pre-shaped abstractions over inventing a new one.

## S — Single Responsibility

A class/module/function has one reason to change.

**Good:**
- One handler per resource: `src/presentation/http/` has one `<entity>_handler.rs` per entity; each owns one entity's HTTP surface.
- One service per entity: `pub type AccountService = GenericCrudService<Account, AccountRepository>;` — the service does CRUD, nothing else.
- One repository per entity, located in `src/infrastructure/persistence/`.

**Violation looks like:**
- A `mega_handler.rs` that handles two or three unrelated entities in one file.
- A `MiscService` with unrelated methods (`send_email`, `compute_tax`, `upload_avatar`).
- Cross-entity SQL in a single repository (joins across aggregates inside one repo method — push to a use case or a read model).

## O — Open / Closed

Open for extension, closed for modification.

**Good:**
- Adding a new entity = new `*_handler.rs` + `*_service.rs` + `*_repository.rs`, with no edits to existing entities. The framework's `BackboneCrudHandler::routes(...)` accepts a fresh service without touching framework code.
- Custom logic that survives regeneration goes through `src/custom.rs` (or `src/custom/`) with `// <<< CUSTOM` guards.

**Violation looks like:**
- Forking a `backbone-*` crate locally and editing it under `modules/*`. (Also breaks the metaphor sync rule — see [`overview.md`](./overview.md).)
- Adding a `match entity_kind { … }` switch in a shared helper instead of letting each entity supply its own behaviour via a trait impl.

## L — Liskov Substitution

Subtypes must be usable wherever the supertype is. For Rust: trait impls must honour the trait's contract.

**Good:**
- Every `Repository<T>` impl in `src/infrastructure/persistence/` returns the same `Result`/`Option` shape the trait promises, no extra preconditions.
- `Future` impls don't panic on `.await`; they return errors through the typed channel.

**Violation looks like:**
- A repository impl that panics on a missing row instead of returning `Ok(None)`.
- A trait method that's `async fn` but the impl blocks on a `std::sync::Mutex` (different real-world behaviour from sibling impls).
- An `update` impl that quietly skips fields if a tenant filter doesn't match — without surfacing the difference to the caller.

## I — Interface Segregation

Many small, focused traits beat one fat trait.

**Good:**
- `backbone-core` splits read and write surfaces (`read_routes` and `write_routes` are independently mountable so read-only data can sit outside auth).
- Service traits with one or two methods are normal; if you find a 10-method trait, ask whether it really has one client or many.

**Violation looks like:**
- A `BigCrudPlusSearchPlusExport` trait that forces every entity to implement all three, even when only CRUD is wanted.
- A repository trait that exposes raw SQL strings (now every caller depends on knowing SQL).

## D — Dependency Inversion

Depend on abstractions, not concretions. Inner layers define the abstractions; outer layers implement them.

**Good:**
- `src/domain/repositories/` defines repository traits in terms of entities and value objects, with no `sqlx`. `src/infrastructure/persistence/` provides the `sqlx`-backed impls.
- [`src/main.rs`](../../src/main.rs) is the **composition root**: it builds the concrete impls and hands them up as trait objects.
- Application services receive a repository (trait) at construction; they never reach for a global pool.

**Violation looks like:**
- An application service that imports `sqlx::PgPool` directly and runs queries.
- A domain entity that takes a `reqwest::Client` to "fetch external data on validate".
- A handler that constructs its own database pool instead of being injected with a service.

## How SOLID maps to the typical change

| Change | Principle most at risk |
|---|---|
| Adding a new entity end-to-end | **O** (extend without modifying), **S** (one folder per layer) |
| Adding a new HTTP-only endpoint over existing service | **S** (don't bloat the service), **I** (don't expand the service trait) |
| Adding cross-cutting behaviour (audit, metrics) | **D** (use middleware / tower layer, don't sprinkle into services) |
| Specialising a repository | **L** (still honour the trait), **I** (split read/write if applicable) |
| Replacing storage (Postgres → ScyllaDB) | **D** (depend on traits, not crates) — if this is painful, your services were importing `sqlx`. |

## Quick checks before submitting

- Open the diff. For each touched file, name the **one** reason it changed. If you can't, it's doing more than one thing.
- `grep -n "use sqlx" src/domain` and `grep -n "use sqlx" src/application` — should return nothing.
- `grep -n "use axum" src/domain` and same for `src/application` — should return nothing.
- For any new trait: is there exactly one caller, and is it inside the same crate? If yes, you probably don't need the trait — inline.
