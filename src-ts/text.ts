import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow"
import { useEffect, useState } from "react"

const TEXT_INPUT_TYPES = new Set([
  "",
  "text",
  "search",
  "url",
  "email",
  "tel",
  "password",
  "number",
])

/** True for elements that have their own typing undo: text inputs, textareas, contenteditable. */
export function isTextField(el: EventTarget | Element | null): boolean {
  if (!(el instanceof HTMLElement)) return false
  if (el instanceof HTMLTextAreaElement) return !el.readOnly && !el.disabled
  if (el instanceof HTMLInputElement)
    return TEXT_INPUT_TYPES.has(el.type) && !el.readOnly && !el.disabled
  return el.isContentEditable
}

/**
 * Whether an editable text field has focus in this window. Checkbox, colour, range and other
 * non-text inputs do not count.
 */
export function useTextFocus(): boolean {
  const [focused, setFocused] = useState(() => isTextField(document.activeElement))
  useEffect(() => {
    const onIn = (e: FocusEvent) => setFocused(isTextField(e.target))
    const onOut = (e: FocusEvent) => setFocused(isTextField(e.relatedTarget))
    // The window losing focus keeps document.activeElement; the menu must not treat it as typing.
    const onWinBlur = () => setFocused(false)
    const onWinFocus = () => setFocused(isTextField(document.activeElement))
    document.addEventListener("focusin", onIn)
    document.addEventListener("focusout", onOut)
    window.addEventListener("blur", onWinBlur)
    window.addEventListener("focus", onWinFocus)
    return () => {
      document.removeEventListener("focusin", onIn)
      document.removeEventListener("focusout", onOut)
      window.removeEventListener("blur", onWinBlur)
      window.removeEventListener("focus", onWinFocus)
    }
  }, [])
  // WebKit fires no focusout when the focused field is removed from the page (a name field
  // that unmounts on Enter), which would leave Undo stuck on the field. While a field has
  // focus, recheck whenever nodes go away.
  useEffect(() => {
    if (!focused) return
    const observer = new MutationObserver(() => {
      if (!isTextField(document.activeElement)) setFocused(false)
    })
    observer.observe(document.body, { childList: true, subtree: true })
    return () => observer.disconnect()
  }, [focused])
  return focused
}

/**
 * Text fields keep their native typing undo. Edit > Undo and Redo are custom menu items, so
 * the menu would swallow Cmd+Z; instead, while a text field has focus:
 *   1. this hook reports `textFocus` (`useCommands` passes it to `useMenuSync`, which sends it);
 *   2. Rust shows plain "Undo" and "Redo" and, on select, emits `text-undo` ("undo" | "redo")
 *      to the focused window instead of touching the document;
 *   3. this hook runs `document.execCommand` on the focused field.
 * Fields commit on blur or Enter as one labelled edit, so the two undo scopes do not overlap.
 * `execCommand` is deprecated but is the only way to drive WebKit's field undo stack.
 */
export function useTextUndo(): boolean {
  const textFocus = useTextFocus()
  useEffect(() => {
    const un = getCurrentWebviewWindow().listen<"undo" | "redo">("text-undo", (e) => {
      document.execCommand(e.payload)
    })
    return () => {
      un.then((f) => f())
    }
  }, [])
  return textFocus
}

/** Commit a focused field (its onBlur) before a document-level undo or redo from a button. */
export function blurField() {
  if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
}
