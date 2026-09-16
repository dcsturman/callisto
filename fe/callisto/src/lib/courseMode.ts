import {CourseMode} from "lib/flightPath";

/**
 * What a plotted course actually promises, in words for a pilot who was not
 * party to how the navigation computer works.
 *
 * The computer tries three things in turn -- a full rendezvous, then a burn
 * through the target's projected position at any speed, then the burn that
 * lets the range grow least -- and never simply refuses. That is only useful if
 * the pilot is told which one they got, because the three mean very different
 * things about what happens next turn.
 */
export const describeCourse = (
  mode: CourseMode | undefined,
  target: string | null,
  blip: boolean,
): {headline: string; detail: string} => {
  const name = target ?? "the target";
  const course = (() => {
    switch (mode ?? "Intercept") {
      case "Intercept":
        return {
          headline: "Intercept",
          detail: `Arrives alongside ${name}, matching its velocity.`,
        };
      case "Pursuit":
        return {
          headline: "Pursuit",
          detail:
            `${name} cannot be matched, so this course crosses its projected ` +
            `position at speed. Re-plot each turn as it moves.`,
        };
      case "Shadow":
        return {
          headline: "Shadowing",
          detail:
            `${name} is pulling away faster than you can close. This burn ` +
            `holds the range as best it can this turn. Re-plot each turn.`,
        };
    }
  })();
  if (!blip) {
    return course;
  }
  return {
    ...course,
    detail:
      `${course.detail} No sensor contact on ${name}: its acceleration is ` +
      `unknown, so the course assumes it holds its current velocity.`,
  };
};
