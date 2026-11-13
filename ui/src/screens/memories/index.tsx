import { useRoute } from "wouter";
import { Placeholder } from "../Placeholder";

/** Serves #/memories, #/memories/:entityId and #/items/:itemId. */
export default function Memories() {
  const [isItem] = useRoute("/items/:itemId");
  const [isEntity] = useRoute("/memories/:entityId");
  if (isItem) return <Placeholder title="screen.item.title" />;
  if (isEntity) return <Placeholder title="screen.entity.title" />;
  return <Placeholder title="screen.memories.title" />;
}
