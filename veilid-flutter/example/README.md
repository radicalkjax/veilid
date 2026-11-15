# veilid_example

Demonstrates how to use the veilid plugin.

## Running on the web

`web/index.html` loads the veilid wasm blob out of `web/wasm/`, which is
gitignored and not populated by a plain `flutter run`. Build and copy it first:

```sh
./scripts/wasm_update.sh          # debug
./scripts/wasm_update.sh release  # release
flutter run -d chrome
```

`../run_integration_tests_web.sh` does this step for you.

## Getting Started

This project is a starting point for a Flutter application.

A few resources to get you started if this is your first Flutter project:

- [Lab: Write your first Flutter app](https://flutter.dev/docs/get-started/codelab)
- [Cookbook: Useful Flutter samples](https://flutter.dev/docs/cookbook)

For help getting started with Flutter, view our
[online documentation](https://flutter.dev/docs), which offers tutorials,
samples, guidance on mobile development, and a full API reference.
