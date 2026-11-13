export function generate(tokens: unknown): string;
export function readTokens(): {
  color: {
    light: Record<string, string>;
    dark: Record<string, string>;
    print: Record<string, string>;
  };
  [key: string]: unknown;
};
