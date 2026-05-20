# DDD — Domain-Driven Design in this codebase

A service built on this skeleton typically composes multiple **bounded contexts**, each owned by an upstream module. The DDD vocabulary in this doc maps to concrete folders so you know where to look — and where to push back.

## Ubiquitous language

Entity names in code map 1:1 to the business glossary. Two illustrative shapes:

- An **accounting** module might name `Account`, `Journal`, `JournalLine`, `Ledger`, `FinancialStatement`.
- An **inventory** module might name `Stock`, `StockBatch`, `StockMovement`, `Warehouse`.

**MUST** keep names aligned with what business stakeholders say. Renaming an entity is a cross-cutting change that touches generated code, migrations, exports, and consumers — coordinate before doing it.

## Bounded contexts

Each context lives in its **owning upstream module**. This service is the integration point, not the source of truth.

Two contexts the framework itself owns and you'll almost always wire in:

| Bounded context | Representative entities | Owner |
|---|---|---|
| Identity | user, role, permission, session | a framework-owned identity module |
| File storage | bucket, file_version, file_lock, file_share | a framework-owned storage module |

Every additional context (orders, payments, inventory, accounting, …) is a separate upstream module the consumer workspace pins via `metaphor.yaml`. Pick the names that match the product's ubiquitous language.

**Where to put feature work:**

- **MUST** change schema, business rules, or invariants in the owning upstream module. Then bump `ref:` in the workspace `metaphor.yaml`, `metaphor sync --update`, regenerate, `metaphor migration run-all`.
- **NEVER** add domain logic to this service's `src/domain/` to "hack around" an upstream gap. That permanently forks behaviour.
- Composition glue (wiring a new module's router, adding a custom orchestrator that spans two contexts) does belong here.

## DDD building blocks → code

### Entities

Objects with identity that persist over time. Live in `src/domain/entity/` (**generated** — do not edit) and `src/domain/entities/` (**hand-written** — add new application-level entities here). See [`layers.md`](./layers.md) for the full generated-vs-hand-written convention.

- **MUST** carry a stable identifier (usually `uuid::Uuid`).
- **MUST** enforce invariants at construction (constructors return `Result`).
- **SHOULD** be `serde::Serialize + Deserialize` so the framework can move them through CRUD plumbing.

### Value objects

Equality-by-value, no identity. Live in `src/domain/value_objects/`.

- **MUST** be immutable. Mutation = return a new value.
- **MUST** validate on construction. Invalid states should be unrepresentable.
- Examples: `Email`, `Money`, `PhoneNumber`, `Address` (when used as a component, not an entity).

### Aggregates

A cluster of entities + value objects with a single root entity. Operations that change the cluster go through the root.

- **MUST** keep aggregates small. If a root manages > ~5 entities, split.
- **MUST** route writes through the root's repository — don't bypass with a sub-entity repo.
- **NEVER** create cross-aggregate transactions in `application/`. Use a domain event (`src/domain/event/`, `src/domain/events/`) and let the subscription handle the other aggregate.

### Repositories

The boundary between domain and storage.

- **Trait** lives in `src/domain/repositories/`. It's part of the domain — it speaks in entities and value objects.
- **Impl** lives in `src/infrastructure/persistence/`. It speaks in SQL and `sqlx`.
- The default impl is the framework's `GenericCrudRepository<Entity>`. **MUST** use it unless you have a documented business reason for a specialised repo (e.g. complex query that doesn't map to generic CRUD).

### Application services / use cases

Thin orchestration over domain + repository.

- Most services are type aliases:
  ```rust
  pub type AccountService = GenericCrudService<Account, AccountRepository>;
  ```
- `src/application/service/` is **generated** (type-alias declarations from schema) — do not edit.
- `src/application/services/` is **hand-written** — put new application services here.
- Multi-step workflows (e.g. "create record → reserve resource → schedule follow-up → notify external party") go in `src/application/use_cases/` (**hand-written**); `src/application/usecases/` is **generated**. The naming heuristic — bare/singular = generated, plural/snake_case = hand-written — is documented in [`layers.md`](./layers.md).
- **MUST NOT** put SQL or HTTP types in services. They speak domain types and call repository traits.

### Domain events

Fire from the aggregate, handle in `application/` or `subscriptions/`.

- Definitions: `src/domain/event/`, `src/domain/events/`.
- Dispatch: the framework's event bus (`backbone-messaging`). Don't reinvent.
- Subscribers: `src/subscriptions/` — keep them thin; delegate to an application service.

## Anti-corruption layer

Cross-context translation lives in `src/integration/context_map.rs`.

- **MUST** translate at the boundary. Never let an identifier from one bounded context flow into another's code as the same type — wrap it.
- **NEVER** make one context import another's entity directly. If you need data, expose it via the source context's `exports/` and translate.

## Bounded-context boundaries (MUST / NEVER)

- **MUST** keep changes within a single bounded context per PR. Cross-context changes need an explicit "this is a cross-context change" note.
- **MUST** prefer upstream module changes over local overrides. Local overrides become permanent forks.
- **NEVER** let presentation layer code reach across contexts. If a handler needs data from two contexts, it composes two application services — it doesn't query two repositories.
- **NEVER** duplicate an entity definition across contexts. If two contexts both need the same concept, one owns it and the other gets a translation in `integration/context_map.rs`.

## When to use the framework primitives vs writing your own

| You need… | Use this first | Hand-roll only if… |
|---|---|---|
| CRUD over an entity | `BackboneCrudHandler` + `GenericCrudService` + `GenericCrudRepository` | The entity needs a non-CRUD operation (e.g. batch settlement, complex search). Then add a custom method on the service, keep CRUD on the framework. |
| Background job | `backbone-jobs` | The job needs framework-internal state. (Rare.) |
| Cache | `backbone-cache` | Per-entity bespoke eviction. |
| Auth | `backbone-auth` + `backbone-authorization` middleware | Custom policy logic — extend, don't replace. |

When in doubt: search the framework crate source for the type you want before writing a new one. The repository search query is usually just the noun (`Repository`, `Handler`, `Service`).
