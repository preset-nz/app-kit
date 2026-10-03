import { useEffect, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow"

import type { Command } from "./commands"

/** Rust owns these: Undo and Redo follow the history, Revert to Saved… the document. */
const RUST_GATED_IDS = new Set(["edit.undo", "edit.redo", "file.revert"])

/**
 * Connects the commands to the native menu: menu items arrive as `command` events carrying
 * the command id, and enabled and checked state goes back through `app_kit_menu_state`.
 * Undo and redo are not sent; Rust sets their titles from the history, or from `textFocus`
 * (see text.ts) while a text field has focus.
 */
export function useMenuSync(commands: Command[], textFocus: boolean) {
  const latest = useRef(commands)
  // Declared before the effects below, so they see the commands of this render.
  useEffect(() => {
    latest.current = commands
  })

  useEffect(() => {
    const un = getCurrentWebviewWindow().listen<string>("command", (e) => {
      const c = latest.current.find((x) => x.id === e.payload)
      if (c?.enabled) c.run()
    })
    return () => {
      un.then((f) => f())
    }
  }, [])

  const key = JSON.stringify(commands.map((c) => [c.id, c.enabled, c.pressed])) + textFocus
  useEffect(() => {
    const states = latest.current
      .filter((c) => !RUST_GATED_IDS.has(c.id))
      .map((c) => ({ id: c.id, enabled: c.enabled, checked: c.pressed }))
    invoke("app_kit_menu_state", { states, textFocus }).catch(() => {})
    // `key` stands for the commands' menu-relevant state; `latest` holds the commands.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key])
}
