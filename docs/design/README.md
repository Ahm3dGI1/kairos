# /docs/design

The seven artboards of **Direction B — "Command Deck"**, extracted from the
Claude Design deck and kept here as the reference the Windows shell is built
against. Each is a standalone HTML page; open one in a browser to see the
intended surface at its real size.

| Board | Shows |
| --- | --- |
| `direction-b-command-deck.html` | The task list: rail, status line, grouped rows, bottom capture bar |
| `b-calendar.html` | The month grid |
| `b-habits.html` | The contribution grid, the month journal, the summary |
| `b-detail-pane.html` | The detail column |
| `b-capture.html` | The capture bar's five states, and the tint legend |
| `b-small-surfaces.html` | Quick-add overlay, sticky note, tray hover, undo toast, the minimum window |
| `b-light-theme.html` | The same task list in light |

The direction in one line: proportional type for what a person wrote,
monospace for everything the machine knows, one amber accent reserved for *now*,
and density instead of decoration.

`windows-app/src/styles.css` carries these as tokens — dark is the default and
light is the override.
