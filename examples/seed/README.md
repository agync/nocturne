# Seed examples

Synthetic text for a first run. Do not commit personal data.

```text
I prefer oat milk.
I decided to ship the paper before the senses.
I am working on Nocturne.
I live in Berlin.
```

Load them with:

```bash
while IFS= read -r line; do
  nocturne remember "$line"
done < examples/seed/notes.txt
nocturne sleep
nocturne paper
```

For a richer demo, add a photo, a voice note, and a page that share one decision. Keep the originals
outside the repository.
