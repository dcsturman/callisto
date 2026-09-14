/**
 * How a results-log line is presented, based on what it reports.
 *
 * The category rides on the wire beside the text (Rust `MessageCategory`) so
 * the log can be coloured without matching English against the message. That
 * matters because the wording changes: every combat line was reworded once
 * already to show its arithmetic, and anything keyed on the old phrasing would
 * have gone quietly grey.
 */
export type MessageCategory =
  | "Attack"
  | "Damage"
  | "Critical"
  | "Detection"
  | "Handoff"
  | "Engineering"
  | "Leadership"
  | "Destruction"
  | "Info";

/**
 * Colours for the results log.
 *
 * Grouped so a turn can be skimmed rather than read: the red family is things
 * breaking, and gets louder as it gets worse; cool colours are information
 * (sensors, sharing); warm non-red is the crew acting. Attacks are the most
 * common line by far and so are the quietest -- if every shot shouted, the
 * critical hit buried among them would not.
 *
 * Chosen against the dark chrome the rest of the UI uses, and kept distinct
 * from {@link TEAM_CSS} in hue so a red line never reads as "team Red".
 */
const CATEGORY_COLOR: Record<MessageCategory, string> = {
  Attack: "#9fb4c7",
  Damage: "#ff8a5c",
  Critical: "#ff4d6d",
  Destruction: "#ff4d6d",
  Detection: "#6bd5ff",
  Handoff: "#5ddc7a",
  Engineering: "#c9a0ff",
  Leadership: "#ffc94d",
  Info: "#e8e8e8",
};

/** The categories loud enough to warrant weight as well as colour. */
const EMPHATIC = new Set<MessageCategory>(["Destruction"]);

/**
 * Style for one results line.
 *
 * Anything unrecognised -- an older server, or a category added ahead of the
 * client -- falls back to the neutral Info styling rather than disappearing.
 */
export const messageStyle = (
  category: string | undefined,
): {color: string; fontWeight?: number} => {
  const known = (category ?? "Info") as MessageCategory;
  const color = CATEGORY_COLOR[known] ?? CATEGORY_COLOR.Info;
  return EMPHATIC.has(known) ? {color, fontWeight: 700} : {color};
};
