# TJXY First-Run Setup Design

Date: 2026-08-04
Status: Approved

## Goal

Add a first-run setup experience that configures a new TJXY installation before
the ordinary client and administrator applications become available. The same
flow must support a directly executed TJXY binary and a container with a mounted
configuration directory.

The setup creates the minimum viable installation:

- system name, subtitle, locale, logo, and icon;
- SQLite, PostgreSQL, or MySQL database connection;
- listen address, port, and optional public URL;
- the first enabled administrator;
- persistent server identity and credential-encryption key material.

Media libraries, metadata providers, AI providers, cloud storage, Redis, and
advanced performance settings remain post-install administrator workflows.

## Existing Constraints

TJXY currently connects to the configured database, runs migrations, and requires
a bootstrap administrator before it can build the production router. Branding and
listen settings already have a durable system-settings model, while the database
URL and server identity are startup inputs. The setup therefore cannot be an
ordinary authenticated administrator page and cannot depend on the selected
database before that database has been validated.

The workspace compiles SeaORM 1.1.14 with SQLite, PostgreSQL, and MySQL drivers.
SQLite is the default, PostgreSQL is a release-gated backend, and MySQL currently
has only an allowed-to-fail database smoke job. This project promotes MySQL to the
same formal support tier and adds the missing application and server coverage.

## Chosen Architecture

Use one TJXY executable with two mutually exclusive runtime modes:

1. **Setup runtime:** selected only when no completed local installation manifest
   exists. It serves the setup frontend, health checks, and a bounded `/Setup/*`
   API. It does not mount login, client, administrator, media, or compatibility
   routes.
2. **Application runtime:** selected after setup commits a completed manifest. It
   follows the existing initialization path and never exposes setup routes.

This avoids a temporary SQLite database, data copying between databases, and a
separate platform-specific installer. Both native and container deployments use
the same browser workflow and configuration schema.

An installed system whose database is unavailable must enter a dedicated startup
failure state. It must never fall back to setup mode because database failure is
not evidence that the installation is new.

## Configuration Model

Configuration precedence is:

1. environment variables;
2. persistent configuration file;
3. built-in defaults.

The native binary uses the platform configuration directory. A container uses a
documented `/config` mount by default. An explicit environment variable may select
another configuration file. Environment overrides remain visible in the ordinary
system-settings page and are not rewritten by setup.

The completed manifest and configuration contain:

- a format version and installation-completed marker;
- persistent server UUID;
- database backend and connection settings;
- network startup settings;
- generated credential-encryption key material;
- references to durable branding assets;
- a timestamp and installation identifier used for recovery and idempotency.

Database and key material are never returned by read APIs. The file is written
atomically and restricted to the service account on native installations. Docker
documentation requires the mounted configuration directory to have equivalent
ownership and permissions. Logs and diagnostics show only masked configuration.

## Access Boundary

The user declined a one-time installation code. Setup access is therefore limited
to loopback or private-network source addresses. Native startup binds the setup
runtime to loopback by default. Container startup may bind inside the container so
the host can reach it, but the setup handlers still enforce the private-source
rule. Forwarded headers are not trusted unless an explicit trusted-proxy policy is
configured.

The setup browser receives a process-local, `SameSite=Strict` session cookie and
must submit CSRF proof for mutations. Database tests and completion attempts have
short timeouts, concurrency bounds, and rate limits. Setup completion permanently
removes the setup router after restart.

## Screens And Flow

The experience has eight screens. Only four are data-entry steps.

### 0. Startup Animation

Reserve a full-screen media slot for the future animation. The initial build may
use a static branded fallback. Users can skip it, animation failure cannot block
setup, and `prefers-reduced-motion` bypasses motion automatically. The animation
does not appear in the step count.

### 1. Welcome And Environment Check

Show the TJXY version, deployment mode, configuration-directory writability,
source-network eligibility, and any blocking environment overrides. A failed
required check has an actionable but secret-free message and prevents continuing.

### 2. Basic Information (Step 1 of 4)

Collect site title, subtitle, interface locale, logo, and application icon. Uploaded
images use the existing bounded image validation and asset-writing rules. The page
supports preview and restoring defaults.

### 3. Database (Step 2 of 4)

Offer SQLite, PostgreSQL, and MySQL as supported choices.

- SQLite selects or creates a server-local database file within an allowed root.
- PostgreSQL and MySQL collect host, port, database, username, password, and TLS
  policy through separate fields rather than asking users to hand-edit a URL.
- Advanced users may reveal a connection-URL field, but the response and UI must
  never echo embedded passwords.
- A bounded connection test reports backend type, server version, and latency.
- Continuing requires a successful test for the unchanged connection draft.

Switching database types preserves a separate in-memory draft for each type during
the current setup session.

### 4. Network (Step 3 of 4)

Collect listen host, port, and optional public URL. Validate syntax, port range,
availability, and public-URL consistency. These settings remain a draft and do not
interrupt the current setup connection. They become active only after completion
restarts the service.

The completion screen must show the exact destination URL whenever the address or
port changes.

### 5. Administrator (Step 4 of 4)

Collect username, password, and password confirmation. Apply the existing username
rules and a documented password policy. Passwords are masked, excluded from browser
persistence, and sent only with the final setup submission over the current origin.

### 6. Review And Install

Show a structured summary of branding, database type and endpoint, network address,
and administrator username. Mask all secrets. Re-run stale preflight checks before
enabling the single primary `Install` command. The user can return to prior steps
until installation starts; after submission the configuration is immutable within
that attempt.

### 7. Progress And Completion

Display ordered installation stages: validating, connecting, migrating, creating
the administrator, writing configuration, restarting, and checking readiness.
Completion offers one command to continue to the login page at the effective URL.
If the service moves to another origin, show the destination explicitly before
navigating.

## HeroUI Experience

The setup lives in the existing React application and uses the installed
`@heroui/react` v3 package, current theme tokens, and current light/dark behavior.
It does not create a parallel component library.

- HeroUI `Input`/`TextField`, `Select`, `RadioGroup`, `Checkbox`, and `FileTrigger`
  implement form controls.
- HeroUI `Button`, `Alert`, `ProgressBar`, `Modal`, and `Skeleton` implement
  commands and states.
- Lucide supplies icons only.
- Desktop uses a fixed left step rail and one unframed form surface.
- Mobile uses a compact top progress indicator and the same field order.
- Stable responsive constraints prevent controls, labels, and pending states from
  resizing or overlapping the layout.
- Keyboard navigation, visible focus, accessible names, field descriptions, and
  field errors follow HeroUI/React Aria composition.

The UI uses one primary action per screen. It does not nest cards, expose raw
driver errors, clear drafts on backend switches, or permit navigation back after
installation begins.

## Setup API

The setup router exposes only these conceptual operations:

- `GET /Setup/Status` returns setup state and safe environment checks.
- `POST /Setup/Database/Test` performs a bounded test without applying migrations.
- `POST /Setup/Network/Validate` validates the proposed listener and public URL.
- `PUT /Setup/Branding/{kind}` validates a temporary logo or icon upload.
- `POST /Setup/Complete` starts one idempotent installation attempt.
- `GET /Setup/Progress` streams safe stage updates using server-sent events.

Every response uses stable error categories. Driver messages, filesystem paths
outside approved display values, connection URLs, passwords, key material, SQL,
and stack traces remain server-side and are redacted from logs.

## Completion And Recovery

Completion proceeds in this order:

1. validate the session, unchanged database test, network draft, branding, and
   administrator input;
2. connect to the selected database with bounded options;
3. run migrations and validate the resulting schema;
4. create or recover the first enabled administrator;
5. persist durable branding and system settings;
6. atomically write the completed local configuration;
7. request graceful restart;
8. probe readiness at the effective address and direct the browser to login.

The completion operation carries an installation identifier and is idempotent.
If the process stops after database initialization but before local configuration
commit, the next setup runtime detects the matching database installation record.
It requires the operator to prove the same administrator credentials before it
adopts and completes that installation. Plaintext passwords are never stored in a
draft or recovery file.

A connection failure before database mutation returns to the database step. A
migration or schema validation failure stops installation and provides a retry or
database-change path without claiming completion. A configuration write or restart
failure retains a recoverable installation state and never silently opens the
ordinary application on partial configuration.

## Database Support Promotion

SQLite, PostgreSQL, and MySQL are formal setup choices only after they meet the
same contract:

- every migration applies and rolls back on a disposable database;
- repository, application, import, and server contract suites pass;
- first-admin creation and setup idempotency pass;
- CI failure blocks release.

The existing MySQL smoke job becomes a required full-stack job. Backend-specific
SQL remains isolated behind existing repository and migration patterns rather than
leaking into setup handlers.

## Testing And Acceptance

Backend tests cover runtime-mode selection, source-network restrictions, CSRF,
rate limits, secret redaction, connection timeouts, backend/version reporting,
network validation, idempotent completion, interruption at every completion stage,
and refusal to reopen setup after installation or database failure.

Database contracts run against SQLite, pinned PostgreSQL, and pinned MySQL. They
cover forward and backward migrations, installation records, administrator
creation, recovery proof, and cleanup of failed attempts.

Frontend tests cover all eight screens, four-step navigation, per-database draft
retention, stale connection-test invalidation, branding validation, password rules,
safe summaries, progress streaming, reconnect behavior, reduced motion, keyboard
operation, focus order, and accessible error announcements.

Playwright validates native-style and container-style journeys at 1440x900,
768x1024, and 390x844 in light and dark themes. No supported viewport may have
document overflow, overlapping controls, clipped text, or a blank animation state.

Setup is accepted when a clean installation can select any supported database,
complete the wizard, restart at the requested address, authenticate as the created
administrator, and observe that every setup endpoint is unavailable afterward.

## Documentation And Delivery Boundaries

Implementation updates the README with native and Docker first-run commands,
configuration locations and precedence, private-network setup limits, database
examples, recovery instructions, and the three-backend support statement. Public
API documentation records setup endpoints and their removal after completion.

The first implementation does not add media-library creation, metadata or AI
provider setup, cloud storage binding, Redis configuration, backup/restore, data
migration between database engines, TLS certificate issuance, or public remote
setup. Those remain separate authenticated administrator or deployment workflows.
