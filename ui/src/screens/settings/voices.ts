/** The voices that ship with the reading-aloud edition. British first, as the default is. */
export const VOICES: ReadonlyArray<{ id: string; name: string }> = [
  { id: "bf_emma", name: "Emma (British)" },
  { id: "bf_isabella", name: "Isabella (British)" },
  { id: "bf_alice", name: "Alice (British)" },
  { id: "bf_lily", name: "Lily (British)" },
  { id: "bm_george", name: "George (British)" },
  { id: "bm_lewis", name: "Lewis (British)" },
  { id: "bm_daniel", name: "Daniel (British)" },
  { id: "bm_fable", name: "Fable (British)" },
  { id: "af_heart", name: "Heart (American)" },
  { id: "af_bella", name: "Bella (American)" },
  { id: "am_michael", name: "Michael (American)" },
];

export const SPEEDS: readonly number[] = [0.8, 0.9, 1, 1.1, 1.25, 1.5];

/** Shows a voice the engine reports even when it is not in the list above. */
export function voiceName(id: string): string {
  const known = VOICES.find((v) => v.id === id);
  if (known) return known.name;
  const bare = id.replace(/^[a-z]{2}_/, "");
  return bare ? bare.charAt(0).toUpperCase() + bare.slice(1) : id;
}
