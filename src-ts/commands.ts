import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react"
import { invoke } from "@tauri-apps/api/core"

import { redo, undo, useHistory, type HistoryState } from "./history"
import { useTextUndo } from "./text"
import { useMenuSync } from "./useMenuSync"

/** A command as Rust declares it (`CommandInfo`): the one place ids, labels and accelerators live. */
export interface CommandSpec {
  id: string
  /** Menu text. */
  label: string
  /** Toolbar text, when it differs from the menu's. */
  toolbarLabel: string | null
  /** Tauri accelerator string, `CmdOrCtrl+Alt+S`. */
  accelerator: string | null
  /** Display form for tooltips, `⌥⌘S`. */
  shortcut: string | null
  menu: "app" | "file" | "edit" | "view" | "domain" | "window" | "help"
  /** The domain menu's title ("Effect"), for `menu: "domain"`. */
  domain: string | null
  /** The submenu of its section ("Add"), if any. */
  submenu: string | null
  /** The op's own category and tags, for the Add submenu, the library and the palette. */
  category: string | null
  tags: string[]
  kind: "item" | "toggle"
}

/** What the app attaches to a command id: what it does, how it looks, when it is available. */
export interface Binding {
  icon?: ReactNode
  /** Only for text that depends on state ("Light mode" / "Dark mode"). Otherwise the table's label shows. */
  label?: string
  /** Default true. */
  enabled?: boolean
  disabledReason?: string
  /** For toggles. */
  pressed?: boolean
  run?: () => void
}

/** A command bound on both sides: the table's text and shortcut, the app's `run`, icon and gating. */
export interface Command {
  id: string
  label: string
  icon: ReactNode
  shortcut?: string
  enabled: boolean
  disabledReason?: string
  /** Present for toggles. */
  pressed?: boolean
  run: () => void
}

/** The command table from Rust. Null until it arrives (a few milliseconds; never in a plain browser tab). */
export function useCommandTable(): CommandSpec[] | null {
  const [table, setTable] = useState<CommandSpec[] | null>(null)
  useEffect(() => {
    invoke<CommandSpec[]>("app_kit_commands").then(setTable, () => {})
  }, [])
  return table
}

/** Undo and redo are built in: their text, gating and action come from the history. */
function historyBindings(h: HistoryState | null): Record<string, Binding> {
  return {
    "edit.undo": {
      label: h?.undoLabel ? `Undo ${h.undoLabel}` : "Undo",
      enabled: h?.undoLabel != null,
      disabledReason: "Nothing to undo",
      run: () => void undo(),
    },
    "edit.redo": {
      label: h?.redoLabel ? `Redo ${h.redoLabel}` : "Redo",
      enabled: h?.redoLabel != null,
      disabledReason: "Nothing to redo",
      run: () => void redo(),
    },
  }
}

/**
 * Bind the table to the app's behaviour by id. Commands with no binding (or no `run`) stay
 * menu-only. For `edit.undo` and `edit.redo` a binding supplies just the icon; the rest is the history's.
 */
export function bindCommands(
  table: CommandSpec[],
  bindings: Record<string, Binding>,
  history: HistoryState | null = null,
): Command[] {
  const builtin = historyBindings(history)
  return table.flatMap((spec) => {
    const b: Binding = { ...bindings[spec.id], ...builtin[spec.id] }
    const bound = spec.id in bindings || spec.id in builtin
    if (!bound || !b.run) return []
    return [
      {
        id: spec.id,
        label: b.label ?? spec.toolbarLabel ?? spec.label,
        icon: b.icon ?? null,
        shortcut: spec.shortcut ?? undefined,
        enabled: b.enabled ?? true,
        disabledReason: b.disabledReason,
        pressed: spec.kind === "toggle" ? (b.pressed ?? false) : b.pressed,
        run: b.run,
      },
    ]
  })
}

/**
 * The app's whole command wiring in one hook: loads the table, binds it, follows the history,
 * routes text-field undo, and keeps the native menu's enabled and checked state in step.
 * `bindings` should be memoised; the commands are rebuilt when it changes.
 */
export function useCommands(bindings: Record<string, Binding>) {
  const table = useCommandTable()
  const history = useHistory()
  const textFocus = useTextUndo()
  const commands = useMemo(
    () => (table ? bindCommands(table, bindings, history) : []),
    [table, bindings, history],
  )
  useMenuSync(commands, textFocus)

  /** Run a command by id, as the menu does: only when enabled. */
  const run = useCallback(
    (id: string) => {
      const c = commands.find((x) => x.id === id)
      if (c?.enabled) c.run()
    },
    [commands],
  )
  /** A command's display shortcut, for anything that shows one (a panel header's tooltip). */
  const shortcut = useCallback((id: string) => table?.find((s) => s.id === id)?.shortcut ?? undefined, [table])
  return { commands, run, shortcut, history }
}
