# How this connects to Leaf

Sprout is a tiny terminal application using Ratatui and Crossterm.
Leaf uses these libraries too, but implements much more functionality.

## Inside the pane

Sprout owns the library and reader you see here.
They are two regions inside ONE terminal pane.
Rex does not know that our left region is a file list.

## Outside the pane

Rex owns its tabs, session sidebar and pane arrangement.
A Rex Lua integration can open a pane running a program like Sprout.
It can also register actions in Rex's command palette.

## Floating layers

Leaf's integration can run Leaf in a floating terminal layer.
Positioning that layer over a pane makes it look like the pane changed.
The original terminal can remain underneath.

## What about real images?

This demo only draws text, borders and background colors.
Leaf can also send images using terminal graphics protocols.
That is an additional capability, beyond Ratatui's ordinary cell grid.

## Your next experiment

Open src/main.rs and find draw_ui.
Change the header text, rebuild, and run it again.
Then change a color in Palette::new.
Finally, add a key to handle_key that changes a field in App.

You have now changed the appearance and behavior of a terminal app.
No modification to Rex or zsh was needed.
