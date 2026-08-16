# Repository Agent Guidelines

## HeroUI React And HeroUI Pro

- Use HeroUI v3 APIs and follow the locally available `heroui-react` and
  `heroui-react-pro` skills plus the HeroUI Pro MCP component documentation.
- Install or update `@heroui-pro/react` with `hpsetup` from
  `https://collectui.youquxing.com/hpsetup/usage`. Do not use the official
  `heroui-pro` CLI.
- Run `hpsetup` from the package that owns the frontend dependency. For this
  repository that is `admin/`, not the repository root. A typical
  non-interactive invocation is:

  ```sh
  HEROUI_KEY=<key> npx -y hpsetup@latest --auto
  ```

  When `HEROUI_KEY` is set, omit the positional `react` argument. In
  `hpsetup@4.7.0`, the first positional argument is parsed as the key; the
  package is detected automatically from `package.json`.

- Never place `HEROUI_KEY` or an `hp_...` key in source files, tracked config,
  shell history examples containing a real value, logs, or command output.
- After setup, verify that `admin/package.json`, `admin/package-lock.json`, and
  `admin/node_modules/@heroui-pro/react` agree on the installed version. Also
  verify the required component exports and `dist/css/index.css` exist.
- Import styles in this order: Tailwind CSS, `@heroui/styles`, then
  `@heroui-pro/react/css`.
- Import OSS components from `@heroui/react` and Pro-only components from
  `@heroui-pro/react`. Do not recreate a Pro component with custom markup when
  the installed package already provides it.

## Server Rebuild And Client Synchronization

- After modifying server code, database migrations, shared API contracts, or
  the web application served by the server, rebuild the affected artifacts and
  restart the local server before handing the task back. Do not assume that
  restarting an existing process recompiles Rust sources.
- For a release-mode local server, run the relevant tests, build
  `admin/dist`, run `cargo build --release -p tjxy-server`, restart
  `target/release/tjxy-server`, and verify both `/System/Ping` and any changed
  route against the newly running process. Confirm that the running executable
  is the newly built binary.
- The Tauri desktop shell in the sibling `TJXY_app` workspace consumes this
  repository's `admin/` frontend. Web UI changes must remain compatible with
  `VITE_TJXY_SHELL=desktop`; test them with `tauri dev`, and rebuild/reinstall
  the desktop bundle when validating an installed application.
- The React Native mobile client in the sibling `TJXY_app/apps/mobile`
  workspace does not consume web React components. Any user-facing feature or
  shared API change that applies to mobile must be implemented and tested in
  the mobile UI and `@tjxy/client-api` separately.
