import { useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"

/** The document as Rust reports it (`DocumentState`): from `app_kit_document`, pushed as `app-kit://document`. */
export interface DocumentState {
  /** `Untitled-1` or the file's name. */
  name: string
  path: string | null
  unsaved: boolean
  /** The window title: the name, then ` *` while unsaved. Rust sets it; shown here for reference. */
  title: string
}

/** The live document state, or null until the first read. The status bar reads `unsaved`. */
export function useDocument(): DocumentState | null {
  const [state, setState] = useState<DocumentState | null>(null)
  useEffect(() => {
    let live = true
    invoke<DocumentState>("app_kit_document").then(
      (s) => live && setState((cur) => cur ?? s),
      () => {},
    )
    const un = listen<DocumentState>("app-kit://document", (e) => setState(e.payload))
    return () => {
      live = false
      un.then((f) => f())
    }
  }, [])
  return state
}
