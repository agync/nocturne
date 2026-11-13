import { useRoute } from "wouter";
import { Browser } from "./Browser";
import { ItemPage } from "./ItemPage";

/** Serves #/memories, #/memories/:entityId and #/items/:itemId. */
export default function Memories() {
  const [isItem, item] = useRoute("/items/:itemId");
  const [isEntity, entity] = useRoute("/memories/:entityId");
  if (isItem && item) return <ItemPage key={item.itemId} id={item.itemId} />;
  return <Browser entityId={isEntity && entity ? entity.entityId : null} />;
}
