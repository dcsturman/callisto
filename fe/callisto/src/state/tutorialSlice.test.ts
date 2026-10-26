import { describe, it, expect } from "vitest";
import reducer, { AppMode, resetServer, setAppMode, setLesson, setShowLessonList } from "state/tutorialSlice";

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

  it("remembers to come back to the lesson list until reset", () => {
    let state = reducer(undefined, setShowLessonList(true));
    state = reducer(state, setAppMode(AppMode.Game));
    expect(state.showLessonList).toBe(true);
    expect(reducer(state, resetServer()).showLessonList).toBe(false);
  });
});
