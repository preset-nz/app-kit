import { invoke } from "@tauri-apps/api/core"

import { isTextField } from "./text"

/**
 * Native context menus only (native-apps.md): right-click opens an app-drawn menu, the short
 * native text menu, or nothing, never WebKit's Look Up / Translate / Inspect Element.
 *  - In an editable text field: WebKit's menu is prevented and `app_kit_text_menu` pops up a
 *    native Undo / Redo / Cut / Copy / Paste / Select All at the pointer.
 *  - Anywhere else: nothing.
 *  - App-drawn menus (the kit's ContextMenu) call preventDefault themselves; this leaves them be.
 * Returns the cleanup.
 */
export function nativeContextMenu(): () => void {
  const onContextMenu = (e: MouseEvent) => {
    if (e.defaultPrevented) return
    e.preventDefault()
    if (isTextField(e.target)) void invoke("app_kit_text_menu")
  }
  document.addEventListener("contextmenu", onContextMenu)
  return () => document.removeEventListener("contextmenu", onContextMenu)
}
