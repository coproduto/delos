# Next features

Features we intend to build (as opposed to performance ideas still to be
investigated, which live in `FUTURE_INVESTIGATIONS.md`).

## Settings menu

A place where the user can adjust how the app uses the machine, persisted
between sessions. Settings to start with:

- **GPU memory for source images.** The budget for keeping source image
  levels resident on the GPU, currently a constant (4 GiB on desktop, 1 GiB
  on mobile: `resident::DEFAULT_BUDGET`). `Resident::set_budget` already
  applies a new value at runtime, evicting least recently used levels if the
  resident ones no longer fit, so the menu only needs to call it and persist
  the value.
- **GPU or CPU rendering.** Today a click on the backend badge in the status
  bar toggles `Engine::use_gpu` for the session only; the menu would make the
  choice explicit and remember it.
- **Cache size.** The engine cache for expensive results (background-removal
  masks) is fixed at 256 MiB (`Cache::new` in `Engine::new`).

Open questions: where settings are stored on each platform, and whether
changes apply immediately (the residency budget can) or on restart.
