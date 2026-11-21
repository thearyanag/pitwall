# From a keypress to pixels

Keep the inspector open while you try the controls.

## 1. Receive an event

Crossterm reads keyboard events from the terminal.
Raw mode makes keys available immediately, without pressing Return.
Try Tab: the inspector will show the decoded key event.

## 2. Update application state

Our Rust struct stores the selected document, scroll position,
focused panel, theme, and whether the inspector is visible.
The handle_key function changes these values.

Press Down with Library focused: selected changes.
Press Down with Reader focused: scroll changes instead.
The same key has a different meaning because focus is state too.

## 3. Build the next frame

draw_ui divides the available rectangle into smaller rectangles.
A List widget fills the library rectangle.
A Paragraph widget fills the document rectangle.
Neither widget is a native macOS view or an HTML element.

## 4. Send terminal instructions

Ratatui compares the new cell buffer with its previous buffer.
Crossterm translates updates into cursor, color and text commands.
These are written to standard output, which is connected to a terminal.

## 5. Render pixels

Rex interprets those terminal instructions.
It renders glyphs using your terminal's font and draws their colors.
Sprout does not call Rex's GPU renderer directly.

## Try the smallest change

Press Tab a few times.
The highlighted border changes, even though the document stays put.
The application describes a whole frame; Ratatui updates changed cells.

## Quit cleanly

q and Ctrl-C leave the alternate screen and disable raw mode.
The terminal's earlier screen becomes visible again.
This app also restores the terminal after a Rust panic.
