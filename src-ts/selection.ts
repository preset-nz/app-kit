// Interaction state, kept out of the document (guidance/design/interaction-state.md): small
// stores, a discriminated union, narrow setters, no `setSelection`. Selection is not an undo
// step: undo and redo leave it alone. A selection whose target is gone must read as none; the
// app knows what exists, so it resolves the item against its document before using it.
//
// An app wraps these in its own narrow setters, `selectLayer = (id) => layer.select({ kind: "layer", id })`,
// and exports those, never the store.

import { createStore, useStore } from "./store"

/** One selected item of type `T`, or `none`. `T` is usually a discriminated union with a `none` member. */
export interface Selection<T> {
  get(): T
  /** Read from a component. */
  use(): T
  select(item: T): void
  /** Back to `none`. */
  clear(): void
  subscribe(listener: () => void): () => void
}

/** A single-select store. `none` is the value that means nothing is selected. */
export function createSelection<T>(none: T): Selection<T> {
  const store = createStore<T>(none)
  return {
    get: store.get,
    use: () => useStore(store),
    select: (item) => store.set(item),
    clear: () => store.set(none),
    subscribe: store.subscribe,
  }
}

/** Several selected items. Opt in where an app has multi-select; most do not. */
export interface MultiSelection<T> {
  get(): readonly T[]
  use(): readonly T[]
  has(item: T): boolean
  /** Replace the selection with one item. */
  select(item: T): void
  /** Replace the selection with these items. */
  selectMany(items: readonly T[]): void
  /** Add the item, or remove it when already selected. */
  toggle(item: T): void
  add(item: T): void
  remove(item: T): void
  clear(): void
  subscribe(listener: () => void): () => void
}

/** A multi-select store. `key` says when two items are the same item. Order is selection order. */
export function createMultiSelection<T>(key: (item: T) => string): MultiSelection<T> {
  const store = createStore<readonly T[]>([])
  const has = (item: T) => store.get().some((x) => key(x) === key(item))
  const without = (item: T) => store.get().filter((x) => key(x) !== key(item))
  return {
    get: store.get,
    use: () => useStore(store),
    has,
    select: (item) => store.set([item]),
    selectMany: (items) => store.set([...items]),
    toggle: (item) => store.set(has(item) ? without(item) : [...store.get(), item]),
    add: (item) => {
      if (!has(item)) store.set([...store.get(), item])
    },
    remove: (item) => store.set(without(item)),
    clear: () => store.set([]),
    subscribe: store.subscribe,
  }
}

/**
 * Where a new item goes (`ux-patterns.md`, menu-standard decision 6): straight after the selected
 * item, or at the end when nothing in `items` is selected. Returns the index to insert at.
 */
export function insertIndex<T>(items: readonly T[], isSelected: (item: T) => boolean): number {
  for (let i = items.length - 1; i >= 0; i--) {
    if (isSelected(items[i])) return i + 1
  }
  return items.length
}
