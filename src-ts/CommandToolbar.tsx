import type { ComponentProps } from "react"
import { ToolbarItems } from "@preset.nz/ux-kit"

import type { Command } from "./commands"
import { toItems, type ToolbarLayout } from "./toolbar"

type Props = Omit<ComponentProps<typeof ToolbarItems>, "groups" | "leading" | "trailing" | "onCommand"> & {
  commands: Command[]
  layout: ToolbarLayout
  /** From `useCommands`: runs the command the menu would. */
  run: (id: string) => void
}

/** ux-kit's `ToolbarItems`, fed from the command table: same labels, shortcuts and gating as the menu. */
export function CommandToolbar({ commands, layout, run, ...props }: Props) {
  return (
    <ToolbarItems
      {...props}
      leading={toItems(commands, layout.leading)}
      groups={layout.groups.map((ids) => toItems(commands, ids))}
      trailing={toItems(commands, layout.trailing)}
      onCommand={run}
    />
  )
}
