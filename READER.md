# Sprout

A small Leaf-inspired terminal reader built to show how a terminal interface works.
Rust + Ratatui + Crossterm. The entire UI lives in one terminal pane.

## Run

Run `./reader` from this folder in Rex or another terminal. The included binary is built for this Mac.

No arguments opens three built-in lessons. To read your own UTF-8 text files:

```sh
./reader README.md src/bin/reader.rs
```

The reader supports headings, bullet markers and code fences, not full Markdown.
It does not edit files, execute document content, fetch URLs, or render PDFs/images.

## Try this sequence

1. Start with the inspector open. Read the first lesson.
2. Press Down: `selected` changes and the next document appears.
3. Press Tab: focus changes from Library to Reader.
4. Press Down again: `scroll` changes instead of the selected document.
5. Press `t`: colors change across the entire app. Rex's outer UI stays its own.
6. Press `i`: the inspector disappears and the document gets more room.
7. Resize the terminal: a resize event triggers layout and wrapping changes.
8. Press `q`: the previous terminal screen returns.

Controls: Tab switches focus; arrows or hjkl navigate; Space/PageDown and PageUp
scroll a page; g/Home goes to the top; G/End goes to the bottom; t toggles theme;
i toggles the inspector; q or Ctrl-C quits. Inspector needs at least 26 rows;
the compact reader needs 64 columns by 18 rows. Mouse control is not implemented.

## Follow the code

Everything is in `src/bin/reader.rs`:

- `App` holds the selected document, scroll, focus and theme as ordinary data.
- `handle_key` changes that data when a key arrives.
- `draw_ui` describes the desired screen using layouts and widgets.
- `run` draws, waits for an input event, updates state, and repeats.
- `main` enters the alternate screen and raw mode, then restores the terminal.

Ratatui builds an in-memory cell buffer and compares it to its previous buffer.
Crossterm emits the cursor, style and text instructions needed to update the terminal.
Rex (or another terminal emulator) turns those instructions into pixels.
The inspector is also a normal widget drawn by the same mechanism.
The draw counter counts calls to draw, not GPU frames or terminal bytes.

This demo is launched as a normal executable. It does not install a Rex plugin
or change your shell configuration. A Lua action could later launch the same
binary in a Rex pane or floating layer.

## Build

Install Rust, then from this folder:

```sh
cargo build --locked
cargo run --locked --bin reader
```

Dependencies are pinned and Cargo.lock is included. The pinned Ratatui release
uses its opt-in `unstable-rendered-line-info` feature to calculate the wrapped
document height and clamp scrolling accurately.

Start experimenting by changing `Palette::new` or the header in `draw_ui`.
Then add a state field and a key handler for it.

References:
- https://docs.rs/ratatui/0.29.0/ratatui/
- https://docs.rs/crossterm/0.28.1/crossterm/event/
- https://github.com/DeepanshuMishraa/leaf
