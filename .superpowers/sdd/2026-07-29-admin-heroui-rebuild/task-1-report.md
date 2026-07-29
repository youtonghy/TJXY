# Task 1 Implementation Report

## Scope

Established the HeroUI v3 and Tailwind CSS v4 foundation while retaining the existing
React Admin, Material UI, and Emotion application screens and dependencies.

## Files Changed

- `admin/package.json`: pinned the required production and development dependencies.
- `admin/package-lock.json`: resolved the exact dependency graph with npm.
- `admin/postcss.config.mjs`: configured Tailwind v4's PostCSS plugin.
- `admin/src/main.tsx`: imports the global stylesheet once before rendering `App`.
- `admin/src/styles.css`: imports Tailwind, HeroUI styles, and the local theme in that
  order; adds global sizing, overflow, focus, skip-link, wrapping, and reduced-motion
  behavior.
- `admin/src/theme.css`: defines the Operational Neutral token contract with a maximum
  large radius of `0.5rem`.
- `admin/src/ui/HeroUiSmoke.test.tsx`: proves the installed HeroUI compound field and
  button render with accessible roles and names.
- `admin/tsconfig.app.json`: adds `vite/client` to the existing explicit ambient type
  list so TypeScript recognizes the required CSS side-effect import.
- `docs/superpowers/specs/2026-07-29-admin-heroui-rebuild-design.md` and
  `docs/superpowers/plans/2026-07-29-admin-heroui-rebuild.md`: tracked approved design
  and plan artifacts.

## Commands And Results

Baseline checks before dependency changes:

```text
npm --prefix admin run typecheck                 PASS
npm --prefix admin run lint                      PASS
npm --prefix admin test -- --run                 PASS: 21 files, 116 tests
npm --prefix admin run build                     PASS: retained mui and react-admin chunks
```

Installed the pinned dependencies:

```text
npm --prefix admin install --save-exact @heroui/react@3.2.2 @heroui/styles@3.2.2 ra-core@5.15.0 react-hook-form@7.82.0 tailwind-variants@3.2.2 lucide-react@1.27.0
Result: added 48 packages

npm --prefix admin install --save-dev --save-exact tailwindcss@4.3.3 @tailwindcss/postcss@4.3.3 postcss@8.5.20 @axe-core/playwright@4.12.1
Result: added 18 packages
```

Focused and final verification:

```text
npm --prefix admin test -- --run src/ui/HeroUiSmoke.test.tsx
PASS: 1 file, 1 test

npm --prefix admin run typecheck
PASS

npm --prefix admin run lint
PASS

npm --prefix admin run build
PASS: emitted index CSS bundle; retained mui and react-admin chunks

npm --prefix admin test -- --run
PASS: 22 files, 117 tests

npm --prefix admin ls ra-core
PASS: direct ra-core@5.15.0 with all React Admin references deduped to that instance

rg -o -- "--color-accent|data-slot" admin/dist/assets/*.css | sort -u
PASS: emitted both --color-accent and data-slot selectors

git diff --check
PASS
```

## Dependency And CSS Verification

`npm ls` reports exactly one installed `ra-core@5.15.0` instance, shared by the direct
dependency and React Admin's transitive packages. The production CSS output contains both
the Operational Neutral accent token and HeroUI component slot selectors, confirming that
Tailwind and HeroUI styles pass through PostCSS and Vite together.

## Self Review

- Used HeroUI v3 compound components and no provider in the smoke test.
- Kept all existing MUI, Emotion, and React Admin dependencies and left application
  presentation code untouched.
- Added only the Vite ambient declaration required by the new CSS import; the project
  deliberately has an explicit TypeScript `types` allowlist, which otherwise excludes
  Vite's stylesheet declarations.
- Avoided an unresolved-import test: configuration and dependency setup is the approved
  TDD exception, while the retained smoke test asserts observable accessible behavior.
- No Rust, backend, route, authentication, or API behavior changed.

## Concerns

- npm reported three high-severity audit findings after installation. This task pins the
  required versions and does not alter or remediate those transitive audit findings; they
  should be assessed separately before release.
- The new all-framework CSS bundle is 409.77 kB before gzip (38.66 kB gzip). It is
  expected for the foundation import but should be watched as future HeroUI slices are
  added.
