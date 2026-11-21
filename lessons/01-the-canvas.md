# Your terminal is a canvas

This entire screen is a Rust program running inside a terminal.
The sidebar, borders and footer are characters with colors.
Rex provides the window; Sprout decides what fills its terminal area.

## Try it

- Press Down in the library to open another lesson.
- Press Tab to focus the document, then Down to scroll.
- Press t to switch the application's colors.
- Press i to hide or show the inspector below.
- Press q to quit and restore the terminal.

## Three different responsibilities

The shell (zsh) launches the program when you type its name.
Sprout handles input and builds the screen it wants to show.
The terminal emulator interprets that output and paints pixels.

## What is a cell?

A terminal area has a number of columns and rows.
Each cell holds a symbol, a foreground color and a background color.
Some symbols, such as emoji, occupy more than one column.

Ratatui gives this program a grid called a buffer.
Widgets write text and styles into that buffer.
Ratatui compares it with the previous buffer and sends the changes.

## This is a real program

The file list is application state, not a terminal feature.
The selected row is an integer in a Rust struct.
Changing that integer causes a different row to be highlighted.

## An experiment

Make the terminal window narrower.
The document wraps and the layout is recalculated.
The terminal sends a resize event; Sprout draws a new frame.

The frame counter does not spin continuously while you sit still.
Sprout blocks waiting for the next input event.
