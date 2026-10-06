import { useEffect, useRef, useState } from "react"
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

/**
 * Notes about the document for the app to show, such as a last document that wouldn't reopen
 * at launch. Each arrives once: the ones held from before the webview listened, then any later
 * `app-kit://note`. Show them as the app shows a passing message (a snackbar), never a dialog.
 */
export function useDocumentNotes(onNote: (message: string) => void): void {
  const handler = useRef(onNote)
  useEffect(() => {
    handler.current = onNote
  }, [onNote])
  useEffect(() => {
    let live = true
    const un = listen<string>("app-kit://note", (e) => handler.current(e.payload))
    // Ask only once listening, so nothing falls between the held notes and the event. And only
    // while still mounted: taking them is once-only, so an unmounted effect (StrictMode mounts
    // twice in dev) must leave them for the next.
    un.then(() => (live ? invoke<string[]>("app_kit_document_note") : [])).then(
      (held) => live && held.forEach((m) => handler.current(m)),
      () => {},
    )
    return () => {
      live = false
      un.then((f) => f())
    }
  }, [])
}
