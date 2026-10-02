/**
 * No browser context menu (native-apps.md): right-click opens an app-drawn menu or nothing,
 * never WebKit's Reload / Inspect Element. Text fields keep the system's Cut / Copy / Paste menu.
 * App-drawn menus (the kit's ContextMenu) handle the event themselves and still open.
 */
export function noWebContextMenu(): void {
  document.addEventListener("contextmenu", (e) => {
    const t = e.target as HTMLElement | null
    const editable = t?.closest("input, textarea, [contenteditable]:not([contenteditable='false'])")
    if (!editable) e.preventDefault()
  })
}
