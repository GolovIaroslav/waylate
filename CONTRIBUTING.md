# Contributing

Waylate is early and intentionally small. Keep changes focused.

## Development

```bash
npm install
npm run check
cd src-tauri && cargo test
```

Run the app:

```bash
npm run tauri dev
```

Build a release binary:

```bash
npm run tauri build -- --no-bundle
```

## Pull requests

- Keep one pull request about one thing.
- Include manual notes for Wayland clipboard, tray, or model changes.
- Do not add large ML dependencies to the desktop app without discussing it first.
