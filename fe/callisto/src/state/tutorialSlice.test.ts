import { describe, it, expect } from "vitest";
import reducer, {
  AppMode,
  increment,
  resetServer,
  setAppMode,
  setLesson,
  setShowLessonList,
} from "state/tutorialSlice";

describe("tutorialSlice", () => {
  // "Next lesson" leaves one lesson and starts the next at once; leaving
  // then switches the mode back to Game after the fact, and used to take
  // the new lesson with it.
  it("keeps the lesson when the mode changes", () => {
    let state = reducer(undefined, setAppMode(AppMode.Tutorial));
    state = reducer(state, setLesson("flying"));
    state = reducer(state, setAppMode(AppMode.Game));
    state = reducer(state, setAppMode(AppMode.Tutorial));
    expect(state.lessonId).toBe("flying");
    expect(state.stepIndex).toBe(0);
  });

  // A reconnect sets the mode again; a lesson in the builder must not start over.
  it("keeps the step when the mode is set to what it already is", () => {
    let state = reducer(undefined, setAppMode(AppMode.ScenarioBuilder));
    state = reducer(state, setLesson("builder"));
    state = reducer(state, increment());
    state = reducer(state, increment());
    state = reducer(state, setAppMode(AppMode.ScenarioBuilder));
    expect(state.stepIndex).toBe(2);
  });

  it("remembers to come back to the lesson list until reset", () => {
    let state = reducer(undefined, setShowLessonList(true));
    state = reducer(state, setAppMode(AppMode.Game));
    expect(state.showLessonList).toBe(true);
    expect(reducer(state, resetServer()).showLessonList).toBe(false);
  });
});
