import { useSyncExternalStore } from "react"

/** A tiny external store: one value, subscribers, no setter wider than the owner chooses to export. */
export interface Store<T> {
  get(): T
  set(next: T): void
  subscribe(listener: () => void): () => void
}

export function createStore<T>(initial: T): Store<T> {
  let state = initial
  const listeners = new Set<() => void>()
  return {
    get: () => state,
    set(next) {
      if (Object.is(next, state)) return
      state = next
      for (const l of listeners) l()
    },
    subscribe(l) {
      listeners.add(l)
      return () => void listeners.delete(l)
    },
  }
}

/** Read a store from a component; it re-renders when the value changes. */
export const useStore = <T>(store: Pick<Store<T>, "get" | "subscribe">): T =>
  useSyncExternalStore(store.subscribe, store.get)
