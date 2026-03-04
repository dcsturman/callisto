export enum ViewMode {
  General,
  Pilot,
  Sensors,
  Gunner,
  Engineer,
  Observer
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
  }
}