import {
  AudioLines,
  BookOpen,
  ChevronDown,
  EyeOff,
  FileText,
  FolderInput,
  Image,
  Lock,
  type LucideIcon,
  Mic,
  Moon,
  Paperclip,
  Pause,
  Play,
  Printer,
  RotateCcw,
  Search,
  Settings,
  SkipForward,
  Sun,
  Trash,
  X,
} from "lucide-react";

/** The only icons Nocturne draws. Always beside a text label, except in the tray. */
const ICONS = {
  mic: Mic,
  play: Play,
  pause: Pause,
  "skip-forward": SkipForward,
  printer: Printer,
  search: Search,
  sun: Sun,
  moon: Moon,
  "folder-inbox": FolderInput,
  image: Image,
  "file-text": FileText,
  "audio-lines": AudioLines,
  lock: Lock,
  "eye-off": EyeOff,
  "rotate-ccw": RotateCcw,
  trash: Trash,
  settings: Settings,
  x: X,
  "chevron-down": ChevronDown,
  paperclip: Paperclip,
  "book-open": BookOpen,
} satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof ICONS;

export const ICON_NAMES = Object.keys(ICONS) as IconName[];

interface IconProps {
  name: IconName;
  size?: number;
  /** Spoken name. Leave out when a text label sits beside the icon. */
  label?: string;
}

export function Icon({ name, size = 20, label }: IconProps) {
  const Glyph = ICONS[name];
  return (
    <Glyph
      size={size}
      strokeWidth={1.5}
      absoluteStrokeWidth
      strokeLinecap="round"
      strokeLinejoin="round"
      data-icon={name}
      {...(label ? { role: "img", "aria-label": label } : { "aria-hidden": true })}
    />
  );
}
