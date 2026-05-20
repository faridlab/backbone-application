# Layers — Clean Architecture in this codebase

This service follows Clean Architecture. The `src/` folder layout already encodes the layers — your job is to keep edits inside the right one.

## The dependency rule

**Dependencies point inward. Inner layers MUST NOT know about outer layers.**

```
┌──────────────────────────────────────────────────────────────┐
│  Frameworks & Drivers       (presentation, middleware,       │
│                              routes, bootstrap, configuration)│
│  ┌────────────────────────────────────────────────────────┐  │
│  │  Interface Adapters       (infrastructure)             │  │
│  │  ┌──────────────────────────────────────────────────┐  │  │
│  │  │  Application Business Rules   (application)      │  │  │
│  │  │  ┌────────────────────────────────────────────┐  │  │  │
│  │  │  │  Enterprise Business Rules   (domain)      │  │  │  │
│  │  │  └────────────────────────────────────────────┘  │  │  │
│  │  └──────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

If you're in `domain/` and you `use sqlx::…` — stop, that's a leak.

## Folder → layer mapping

| Folder | Layer | Responsibility |
|---|---|---|
| `src/domain/` | Enterprise Business Rules | Entities, value objects, repository **traits**, state machines, specifications, domain events. Pure Rust. |
| `src/application/` | Application Business Rules | Use cases, DTOs, application services (mostly `type X = GenericCrudService<…>` aliases), workflows, validators. |
| `src/infrastructure/` | Interface Adapters | Repository **impls** (wrapping `backbone-orm`), persistence, cache, messaging, event store, external clients. |
| `src/presentation/` | Frameworks & Drivers | HTTP/gRPC/GraphQL handlers, CLI entry, wire DTOs, versioning. |
| `src/middleware/` | Frameworks & Drivers | Tower layers — auth, CORS, rate-limit, security headers. |
| `src/routes/` | Frameworks & Drivers | Router composition (entity routers → versioned API). |
| `src/bootstrap/` | Frameworks & Drivers | Initialisation helpers (e.g. module wiring). |
| [`src/configuration/`](../../src/configuration/) + [`config/application*.yml`](../../config/) | Frameworks & Drivers | Config loading and environment binding (loader + YAML overlays). |
| [`src/shared/`](../../src/shared/) | Cross-cutting helpers | `AppState`, error envelope, pagination, response shape. Leaf — no inbound deps from anyone. |
| `src/exports/` | Module boundary | Re-exports for inter-module consumers. Stable surface, treat as public API. |
| `src/integration/` | Anti-corruption layer | `context_map.rs` translates cross-bounded-context types. |
| `src/seeders/` | Support | One folder per entity; data fixtures. Not domain logic. |
| `src/subscriptions/` | Support | Event subscribers / async listeners. |
| `src/custom.rs` / `src/custom/` | Support | Mount point for `// <<< CUSTOM` extensions that survive regeneration. |
| `src/handlers/` | Cross-cutting | Custom orchestrators that don't fit a single entity handler. Use sparingly. |

> Not every folder above ships with the skeleton — `src/domain/`, `src/application/`, `src/infrastructure/persistence/`, `src/presentation/`, `src/routes/`, `src/bootstrap/`, `src/exports/`, `src/integration/`, `src/seeders/`, `src/subscriptions/`, `src/custom*` and `src/handlers/` materialise once you add a module or run `metaphor schema generate`. The starter ships [`src/main.rs`](../../src/main.rs), [`src/configuration/`](../../src/configuration/), [`src/infrastructure/database/`](../../src/infrastructure/database/), [`src/middleware/`](../../src/middleware/), and [`src/shared/`](../../src/shared/) only.

## Where the typical building blocks live

| Building block | Location |
|---|---|
| Entity types (enums, structs) | `src/domain/entity/`, `src/domain/entities/` |
| Value objects | `src/domain/value_objects/` |
| Repository **traits** | `src/domain/repositories/` |
| Repository **impls** | `src/infrastructure/persistence/` |
| Application services (type aliases) | `src/application/service/` |
| Hand-written application services | `src/application/services/` |
| DTOs (request/response payloads) | `src/application/dto/` and `src/presentation/dto/` |
| HTTP handlers | `src/presentation/http/` |
| gRPC handlers | `src/presentation/grpc/` |
| GraphQL resolvers | `src/presentation/graphql/` |

> **Generated vs hand-written folders.** Several layers have parallel folders where one is regenerated from schema and the other is hand-written. The naming convention is consistent — generated uses the bare/singular form, hand-written uses the idiomatic plural / snake_case form:
>
> | Generated (do not edit)        | Hand-written                          |
> |--------------------------------|---------------------------------------|
> | `src/domain/entity/`           | `src/domain/entities/`                |
> | `src/application/service/`     | `src/application/services/`           |
> | `src/application/usecases/`    | `src/application/use_cases/`          |
>
> The definitive check: open the folder's `mod.rs`. Generated ones start with `//! Generated by metaphor-schema. Do not edit manually.` (or `backbone-schema`). Hand-written ones have a descriptive non-generator header. **MUST** add new files to the hand-written folder; edits to the generated folder are wiped on next regeneration.
>
> **Hand-written files inside generator-owned folders.** Sometimes a hand-written file genuinely belongs next to generated ones (e.g. a custom service inside `src/application/service/` that has no schema model). In that case, list its exact path or a glob under `user_owned:` in [`metaphor.codegen.yaml`](../../metaphor.codegen.yaml) and the generator will skip it wholesale — never read, merged, or written. See *Regen safety* in [`CLAUDE.md`](../../CLAUDE.md) for the three preservation mechanisms (user_owned manifest, `// <<< CUSTOM` markers, migration headers).

## HTTP request flow

For a typical `BackboneCrudHandler`-backed endpoint:

```
HTTP request
    │
    ▼
src/main.rs                       ← Axum router boot, tower middleware stack applied
    │
    ▼
src/middleware/*                  ← cors → auth → rate-limit → security headers → logging
    │
    ▼
src/routes/mod.rs                 ← versioned mount: /api/v1/<entity>
    │
    ▼
src/presentation/http/<entity>_handler.rs
    │   (BackboneCrudHandler::routes(service, "/<entity>"))
    ▼
src/application/service/<entity>_service.rs
    │   (type alias over GenericCrudService<Entity, Repository>)
    ▼
src/domain/repositories/<entity>_repository.rs    ← trait
    │
    ▼
src/infrastructure/persistence/<entity>_repository.rs    ← impl wrapping backbone-orm
    │
    ▼
PostgreSQL
    │
    ▼
DTO → ApiResponse<T> → JSON → tower middleware → HTTP response
```

The custom helpers under `src/handlers/` sit between presentation and application when an endpoint needs orchestration across multiple services (e.g. a file upload that touches storage + an entity).

## Layer rules (MUST / NEVER)

### domain/

- **MUST** be pure Rust. Standard library + serde + chrono + uuid + `backbone-core` traits only.
- **NEVER** import `sqlx`, `axum`, `tonic`, `reqwest`, or any infrastructure crate.
- **NEVER** depend on `application/`, `infrastructure/`, `presentation/`. It's the innermost ring.
- **MUST** declare repository **traits** here; impls live in `infrastructure/`.
- **SHOULD** put invariants in constructors / state machines, not in services.

### application/

- **MUST** depend only on `domain/` and `shared/` (plus framework traits like `CrudService`).
- **NEVER** import `sqlx` or write SQL. Go through a repository trait.
- **NEVER** import `axum`/`tonic` request types. Use plain DTOs.
- **MUST** keep services thin. If you're writing > ~100 lines of orchestration, that's a use case — put it under `application/use_cases/` (or `usecases/`, whichever already exists for the bounded context).

### infrastructure/

- **MUST** implement domain traits, not invent new public ones.
- **MUST** keep `sqlx`/HTTP-client/Kafka details contained here.
- **NEVER** be imported from `domain/` or `application/` (except via trait objects passed in at composition time in `main.rs`).

### presentation/

- **MUST** convert wire format ↔ DTO and call into an application service. Nothing else.
- **NEVER** call repositories or `sqlx` directly.
- **NEVER** put validation logic that belongs in a domain invariant. (Surface-level type/format checks are fine.)
- **SHOULD** use `BackboneCrudHandler` for any standard CRUD surface. Hand-rolled routers need a written justification — see [`solid.md`](./solid.md).

### middleware/, routes/, bootstrap/, configuration/

- Composition only. **NEVER** business logic. **NEVER** SQL.
- Config secrets MUST come from env / config loader; **NEVER** hardcoded.

### shared/

- Helpers consumed by any layer. **MUST NOT** import any specific layer's types.
- Error envelope and response shape live here so every layer sees the same types.

### exports/

- The **public** boundary for inter-module consumers. Changing it is a breaking change.
- **MUST** be small. If it's growing, that's a sign the bounded context is too coupled.

### seeders/, subscriptions/, custom/

- Seeders: data only, no domain logic.
- Subscriptions: thin handlers that delegate to an application service.
- Custom: extensions that survive code regeneration. Follow the `// <<< CUSTOM` guard pattern — see `apps-maintainer` / `custom-logic-specialist` skills if you need depth.

## Common cross-layer mistakes (don't make these)

- Putting a DTO in `domain/` "because it's shared". DTOs are application or presentation concerns; domain has entities and value objects.
- Implementing a repository trait inside `application/`. Move it to `infrastructure/persistence/`.
- Importing `sqlx::PgPool` into a service. The pool stays in `infrastructure/`; services receive a repository trait object.
- Hand-rolling `axum::Router::new().route(...)` for an entity that already has a `BackboneCrudHandler`. Re-mount via `BackboneCrudHandler::routes(...)` instead.
- Adding business rules to a middleware. Middleware is for transport concerns (auth, rate-limit, headers); business goes in `application/` or `domain/`.

## Verification before merging

A PR that adds or changes a feature should pass:

```bash
metaphor lint check          # clippy + fmt + audit
metaphor dev test            # unit + integration
metaphor dev test --integration-only
```

If clippy complains about a layering violation (e.g. unused crate in `domain/`), don't `#[allow(...)]` it — fix the layer.
