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
  /** Opens a file or folder with the system's own app. */
  openPath(path: string): Promise<void>;
  /** A URL the page can show for a kept original on disk. */
  originalSrc(path: string): string;
  /** Closes the floating quick-capture window (or leaves the screen, in a browser). */
  closeQuickCapture(): Promise<void>;
  /** Sends a question to the main window, which shows the answer. */
  handOffAsk(question: string): Promise<void>;
  /** Hears questions handed off from the quick-capture window. */
  onAskHandOff(cb: (question: string) => void): Unsubscribe;
}
