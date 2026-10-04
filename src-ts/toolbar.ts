import type { ToolbarItemSpec } from "@preset.nz/ux-kit"

import type { Command } from "./commands"

// The ux-kit half of the command wiring. Kept apart from `commands.ts` so the `./core` entry
// never reaches ux-kit, for apps that only use the menu half (Strata, Fault).

/** The toolbar is a subset of the commands: these ids, in this order, in these groups. */
export interface ToolbarLayout {
  leading?: string[]
  groups: string[][]
  trailing?: string[]
}

export function toItems(commands: Command[], ids: string[] = []): ToolbarItemSpec[] {
  return ids.flatMap((id) => {
    const c = commands.find((x) => x.id === id)
    if (!c) return []
    return [
      {
        id: c.id,
        label: c.label,
        icon: c.icon,
        shortcut: c.shortcut,
        enabled: c.enabled,
        disabledReason: c.disabledReason,
        pressed: c.pressed,
      },
    ]
  })
}
