// The menu half of app-kit: commands, history, text-field undo, selection and stores, with
// no dependency on ux-kit. `index.ts` adds the toolbar bridge on top.
export {
  bindCommands,
  useCommands,
  useCommandTable,
  type Binding,
  type Command,
  type CommandSpec,
} from "./commands"
export { useDocument, type DocumentState } from "./document"
export { redo, undo, useHistory, type HistoryState } from "./history"
export { nativeContextMenu } from "./nativeContextMenu"
export {
  createMultiSelection,
  createSelection,
  insertIndex,
  type MultiSelection,
  type Selection,
} from "./selection"
export { createStore, useStore, type Store } from "./store"
export { blurField, isTextField, useTextFocus, useTextUndo } from "./text"
export { useMenuSync } from "./useMenuSync"
