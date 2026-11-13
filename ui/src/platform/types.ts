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
}
