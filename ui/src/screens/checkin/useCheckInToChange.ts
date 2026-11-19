import { useQuery } from "@tanstack/react-query";
import type { CheckIn } from "../../engine/bindings";
import { useEngine } from "../../engine/context";

/**
 * The check-in that "Change" was pressed on. It is nearly always today's, so today is looked at
 * first, then the other days from the newest back.
 */
export function useCheckInToChange(id: string | null) {
  const engine = useEngine();
  return useQuery({
    queryKey: ["journal", "checkin", id],
    enabled: id !== null,
    queryFn: async (): Promise<CheckIn | null> => {
      const today = await engine.getJournalDay();
      const here = today.checkins.find((c) => c.id === id);
      if (here) return here;
      const days = await engine.listJournalDays();
      for (const day of [...days].reverse()) {
        if (day === today.day) continue;
        const found = (await engine.getJournalDay(day)).checkins.find((c) => c.id === id);
        if (found) return found;
      }
      return null;
    },
  });
}
