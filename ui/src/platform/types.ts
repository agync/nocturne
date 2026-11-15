export type Unsubscribe = () => void;
export type SystemTheme = "light" | "dark";

/** What the interface needs from the window it lives in. Tauri and the browser each implement it. */
export interface Platform {
  /** Files dragged onto the window. `onHover` gets how many are over it, or null when they leave. */
  onFileDrop(
    onDrop: (paths: string[]) => void,
    onHover: (count: number | null) => void,
  ): Unsubscribe;
  onThemeChanged(cb: (theme: SystemTheme) => void): Unsubscribe;
  currentTheme(): Promise<SystemTheme | null>;
  pickFiles(): Promise<string[]>;
  /** Asks the person for a folder; null when they cancel. */
  pickFolder(): Promise<string | null>;
  /** Opens the folder an export just made, with the system's own app. */
  openPath(path: string): Promise<void>;
  /**
   * Opens the kept original of an item with the system's own app. `shown` is the path the page
   * shows it from, which is all the browser preview has to open.
   */
  openOriginal(itemId: string, shown: string): Promise<void>;
  /** Opens the folder of log files. */
  openLogs(): Promise<void>;
  /** A URL the page can show for a kept original on disk. */
  originalSrc(path: string): string;
  /** True in the floating quick-capture window, by the window's own label; never in a browser. */
  inQuickCapture(): boolean;
  /** Closes the floating quick-capture window (or leaves the screen, in a browser). */
  closeQuickCapture(): Promise<void>;
  /** Sends a question to the main window, which shows the answer. */
  handOffAsk(question: string): Promise<void>;
  /** Hears questions handed off from the quick-capture window. */
  onAskHandOff(cb: (question: string) => void): Unsubscribe;
  /** Shows a screen (`/items/abc`) in the main window, bringing it forward. In a browser it is just a move. */
  openInMain(path: string): Promise<void>;
  /** Hears screens the quick-capture window asked the main window to show. */
  onOpenInMain(cb: (path: string) => void): Unsubscribe;
  /** Opens the system's print dialog for the screen being shown. */
  print(): Promise<void>;
}
