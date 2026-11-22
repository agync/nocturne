// A thin promise layer over IndexedDB for what the phone keeps: its device record (the key, the
// id, the computer's key and address), the keeps waiting to go across, and a phone alone's own
// notes (when keeping began, that the lost-phone reminder was shown). Session keys, counters
// and ids are never written here or anywhere else.

export const DB_NAME = "nocturne-phone";
export const DB_VERSION = 1;
export const STORE_DEVICE = "device";
export const STORE_QUEUE = "queue";
/** A phone alone's own notes, in a database of their own so the one above never changes. */
export const NOTES_DB = "nocturne-phone-notes";
export const STORE_NOTES = "notes";

export function openDb(factory: IDBFactory = indexedDB): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = factory.open(DB_NAME, DB_VERSION);
    open.onupgradeneeded = () => {
      const db = open.result;
      if (!db.objectStoreNames.contains(STORE_DEVICE)) db.createObjectStore(STORE_DEVICE);
      if (!db.objectStoreNames.contains(STORE_QUEUE)) {
        db.createObjectStore(STORE_QUEUE, { keyPath: "clientId" });
      }
    };
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error ?? new Error("could not open storage"));
    open.onblocked = () => reject(new Error("storage is blocked"));
  });
}

export function openNotesDb(factory: IDBFactory = indexedDB): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = factory.open(NOTES_DB, 1);
    open.onupgradeneeded = () => {
      if (!open.result.objectStoreNames.contains(STORE_NOTES))
        open.result.createObjectStore(STORE_NOTES);
    };
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error ?? new Error("could not open storage"));
    open.onblocked = () => reject(new Error("storage is blocked"));
  });
}

/** Runs one request in its own transaction and resolves once the transaction has committed. */
export function run<T>(
  db: IDBDatabase,
  store: string,
  mode: IDBTransactionMode,
  op: (s: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  return new Promise((resolve, reject) => {
    let tx: IDBTransaction;
    try {
      tx = db.transaction(store, mode);
    } catch (e) {
      return reject(e);
    }
    let result: T;
    const request = op(tx.objectStore(store));
    request.onsuccess = () => {
      result = request.result;
    };
    tx.oncomplete = () => resolve(result);
    tx.onerror = () => reject(tx.error ?? request.error ?? new Error("storage failed"));
    tx.onabort = () => reject(tx.error ?? new Error("storage was cancelled"));
  });
}
