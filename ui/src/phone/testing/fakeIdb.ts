// Just enough of IndexedDB for the phone's key store and queue to run under test: named
// databases of object stores with get, put, delete, getAll and clear, values cloned as the
// real one clones them, and a transaction that completes after its requests. A real browser
// runs the same code in the end-to-end tests.

interface StoreDef {
  keyPath: string | null;
  rows: Map<string, unknown>;
}

class FakeRequest<T> {
  result!: T;
  error: DOMException | null = null;
  onsuccess: (() => void) | null = null;
  onerror: (() => void) | null = null;
}

class FakeTransaction {
  oncomplete: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onabort: (() => void) | null = null;
  error: DOMException | null = null;
  private pending = 0;
  private done = false;

  constructor(private readonly stores: Map<string, StoreDef>) {
    queueMicrotask(() => this.settle());
  }

  objectStore(name: string) {
    const def = this.stores.get(name);
    if (!def) throw new DOMException("no such store", "NotFoundError");
    const key = (value: unknown, explicit?: IDBValidKey): string => {
      const k = def.keyPath ? (value as Record<string, unknown>)[def.keyPath] : explicit;
      return String(k);
    };
    const op = <T>(fn: () => T): IDBRequest<T> => {
      const request = new FakeRequest<T>();
      this.pending += 1;
      setTimeout(() => {
        request.result = fn();
        request.onsuccess?.();
        this.pending -= 1;
        this.settle();
      }, 0);
      return request as unknown as IDBRequest<T>;
    };
    return {
      get: (k: IDBValidKey) => op(() => structuredClone(def.rows.get(String(k)))),
      put: (value: unknown, k?: IDBValidKey) =>
        op(() => {
          def.rows.set(key(value, k), structuredClone(value));
          return key(value, k);
        }),
      delete: (k: IDBValidKey) =>
        op(() => {
          def.rows.delete(String(k));
          return undefined;
        }),
      getAll: () => op(() => [...def.rows.values()].map((v) => structuredClone(v))),
      clear: () =>
        op(() => {
          def.rows.clear();
          return undefined;
        }),
    } as unknown as IDBObjectStore;
  }

  private settle(): void {
    if (this.done || this.pending > 0) return;
    this.done = true;
    setTimeout(() => this.oncomplete?.(), 0);
  }
}

class FakeDatabase {
  readonly stores = new Map<string, StoreDef>();
  readonly objectStoreNames = { contains: (n: string) => this.stores.has(n) };
  createObjectStore(name: string, options?: { keyPath?: string }) {
    this.stores.set(name, { keyPath: options?.keyPath ?? null, rows: new Map() });
  }
  transaction(_names: string | string[], _mode?: string) {
    return new FakeTransaction(this.stores) as unknown as IDBTransaction;
  }
}

export function fakeIndexedDb(): IDBFactory {
  const databases = new Map<string, FakeDatabase>();
  return {
    open(name: string) {
      const request = {
        result: undefined as unknown,
        error: null,
        onupgradeneeded: null as (() => void) | null,
        onsuccess: null as (() => void) | null,
        onerror: null as (() => void) | null,
        onblocked: null as (() => void) | null,
      };
      setTimeout(() => {
        let db = databases.get(name);
        const fresh = !db;
        if (!db) {
          db = new FakeDatabase();
          databases.set(name, db);
        }
        request.result = db;
        if (fresh) request.onupgradeneeded?.();
        request.onsuccess?.();
      }, 0);
      return request as unknown as IDBOpenDBRequest;
    },
  } as unknown as IDBFactory;
}
