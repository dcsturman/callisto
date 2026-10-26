import { createScenario, exit_scenario } from "lib/serverManager";
import { Lesson } from "lib/tutorial/lesson";
import { generateUniqueHyphenatedName, TUTORIAL_PREFIX } from "components/scenarios/ScenarioManager";
import { resetState, store } from "state/store";
import { AppMode, setAppMode, setLesson, setShowLessonList } from "state/tutorialSlice";
import { setJoinedScenario } from "state/userSlice";

/** Start a lesson in a fresh, private copy of its scenario. */
export function launchLesson(lesson: Lesson) {
  store.dispatch(resetState());
  store.dispatch(setAppMode(AppMode.Tutorial));
  store.dispatch(setLesson(lesson.id));
  createScenario(TUTORIAL_PREFIX + generateUniqueHyphenatedName(), lesson.scenario);
}

/** Leave the lesson's scenario, back to the lesson list. */
export function leaveLesson() {
  store.dispatch(setShowLessonList(true));
  store.dispatch(setJoinedScenario(null));
  exit_scenario();
}
