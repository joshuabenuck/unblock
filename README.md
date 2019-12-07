# Unblock Me!

This is a clone of a game which is known by several different names (Unblock Me!, Traffic, and Rush Hour).

The goal is to get the red block through the yellow exit by moving the other blocks out of the way. Blocks can only be move left / right or up / down along their longest dimension (like a car).

The game can only be controlled with the mouse and does not work on mobile devices due to lack of touch event support.

Written in Rust and compiled to Web Assembly, the game only needs a capable web browser. There are no server side components.

Keybindings:
* `r` - Reset the current level
* `n` - Skip to the next level
* `p` - Go to the previous level

Levels are contained in `levels.dat`.

Each level is a 6x6 grid. All blocks are represented by an ASCII character.
* `&`: An outer wall. All levels should be surrounded by them.
* `^`: The exit. This is the goal. This should be parallel to the red block.
* `=`: The player. This should be two blocks wide and placed parallel to the exit.
* `|`: A vertical block. These should be two to three blocks tall.
* `(`: Second representation of a vertical block. This allows two vertical blocks to be in line with one another.
* `-`: A horizontal block. These should be two to three blocks wide.
* `_`: Second representation of a horizontal block. This allows two horizontal blocks to be in line with one another.

The level parser is very rudimentary.
* Lines can end with newlines and carriage returns.
* Levels may have a comment immediately before them.
* Comments are delimited by a line starting with a `#`.
* Comments are not supported anywhere else.
* The parser stops when the number of remaining characters in the file is not enough to contain a full level. This means the data file can contain some amount of garbage at the end.