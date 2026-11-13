// Runs once before the workers start, so every test sees the same zone and locale.
export default function setup() {
  process.env.TZ = "Europe/London";
  process.env.LANG = "en-GB";
}
