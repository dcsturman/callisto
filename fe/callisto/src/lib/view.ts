export enum ViewMode {
  General,
  Pilot,
  Sensors,
  Gunner,
  Engineer,
  Observer,
  Captain,
}

export function stringToViewMode(role: string) {
  switch (role) {
    case "General":
      return ViewMode.General;
    case "Pilot":
      return ViewMode.Pilot;
    case "Sensors":
      return ViewMode.Sensors;
    case "Gunner":
      return ViewMode.Gunner;
    case "Engineer":
      return ViewMode.Engineer;
    case "Observer":
      return ViewMode.Observer;
    case "Captain":
      return ViewMode.Captain;
  }
}

/**
 * The stations a player may combine. General is every station at once and
 * Observer is none, so neither belongs in a multiple choice.
 */
export const CUSTOMISABLE_ROLES: ViewMode[] = [
  ViewMode.Captain,
  ViewMode.Pilot,
  ViewMode.Sensors,
  ViewMode.Gunner,
  ViewMode.Engineer,
];

/**
 * Whether the player is working any of the given stations.
 *
 * Every role check goes through here, so a player covering two seats on a
 * small crew sees both panels and nothing has to know how many roles there
 * are. General counts as every station, which is what it has always meant.
 */
export const hasRole = (roles: ViewMode[], ...wanted: ViewMode[]): boolean =>
  wanted.some((w) => roles.includes(w)) ||
  (roles.includes(ViewMode.General) && wanted.some((w) => w !== ViewMode.Observer));

/**
 * The referee: running the whole board rather than any one ship. General
 * with no ship, which is the same test the server applies to a reset.
 */
export const isReferee = (roles: ViewMode[], shipName: string | null): boolean =>
  shipName == null && roles.includes(ViewMode.General);

/** "Captain, Gunner" -- names in the order they were chosen. */
export const rolesToString = (roles: ViewMode[]): string =>
  roles.map((r) => ViewMode[r]).join(", ");

/**
 * Roles as sent on the wire and stored. Accepts the older single-role shape
 * too, so a user list from a server one deploy behind still reads.
 */
export const parseRoles = (raw: unknown): ViewMode[] => {
  const list = Array.isArray(raw) ? raw : raw == null ? [] : [raw];
  const parsed = list
    .map((r) => (typeof r === "string" ? stringToViewMode(r) : undefined))
    .filter((r): r is ViewMode => r !== undefined);
  return parsed.length > 0 ? parsed : [ViewMode.General];
};

