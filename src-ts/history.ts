import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { useEffect, useState } from "react"

import { blurField } from "./text"

/** The history as Rust reports it (`HistoryState`): from the `app_kit_history` command, pushed as `app-kit://history`. */
export interface HistoryState {
  undoLabel: string | null
  redoLabel: string | null
  /** Steps done. */
  historyLen: number
  /** Oldest first. */
  undoLabels: string[]
  /** Next first. */
  redoLabels: string[]
}

/** The live history, or null until the first read. */
export function useHistory(): HistoryState | null {
  const [state, setState] = useState<HistoryState | null>(null)
  useEffect(() => {
    let live = true
    invoke<HistoryState>("app_kit_history").then(
      (s) => live && setState((cur) => cur ?? s),
      () => {},
    )
    const un = listen<HistoryState>("app-kit://history", (e) => setState(e.payload))
    return () => {
      live = false
      un.then((f) => f())
    }
  }, [])
  return state
}

/** Undo one step, the one the menu item takes. Blurs a focused field first so its text commits as an edit. */
export function undo(): Promise<void> {
  blurField()
  return invoke<void>("app_kit_undo")
}

export function redo(): Promise<void> {
  blurField()
  return invoke<void>("app_kit_redo")
}
