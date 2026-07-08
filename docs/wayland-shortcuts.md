# Wayland shortcut notes

Waylate targets Wayland, but avoids promising one universal global shortcut
implementation for every compositor.

## KDE Plasma

Use a command shortcut:

```bash
waylate translate-selection
```

KDE launches the command, Waylate reads `wl-paste --primary`, and the running
tray process receives the request through the single-instance plugin.

## Hyprland

Add a bind similar to:

```ini
bind = SUPER SHIFT, T, exec, waylate translate-selection
```

Hyprland users may prefer clipboard-based workflows depending on their seat and
selection settings:

```ini
bind = SUPER SHIFT, Y, exec, waylate translate-clipboard
```

## Sway

Add a binding:

```ini
bindsym $mod+Shift+t exec waylate translate-selection
```

## Why not portal hotkeys first?

The XDG Desktop Portal GlobalShortcuts API is the right long-term direction,
but compositor support and user consent flows still vary. The KDE command
shortcut path is boring, visible to the user, and works today.
