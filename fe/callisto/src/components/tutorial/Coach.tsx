import * as React from "react";
import { useEffect, useMemo, useRef } from "react";
import { CornerPanel } from "lib/CornerPanel";
import { TutorialContext } from "lib/tutorial/lesson";
import { lessonById, nextLesson } from "lib/tutorial/lessons";
import { launchLesson, leaveLesson } from "lib/tutorial/launch";
import { markFinished } from "lib/tutorial/progress";
import { useAppDispatch, useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";
import { decrement, increment } from "state/tutorialSlice";
import { useHighlight } from "components/tutorial/useHighlight";

/** How long a done step stays ticked before the next one. */
const ADVANCE_MS = 1200;

/**
 * The tutorial's coach: one step of the current lesson at a time, docked in
 * the right-hand column. A step that asks for something waits for the game to
 * show it done, ticks, and moves on by itself; Back and Skip are always there
 * so a step can never strand anyone.
 */
export function Coach() {
  const dispatch = useAppDispatch();
  const lessonId = useAppSelector((state) => state.tutorial.lessonId);
  const stepIndex = useAppSelector((state) => state.tutorial.stepIndex);
  const entities = useAppSelector(entitiesSelector);
  const ui = useAppSelector((state) => state.ui);
  const user = useAppSelector((state) => state.user);
  const now: TutorialContext = useMemo(() => ({ entities, ui, user }), [entities, ui, user]);

  const lesson = lessonById(lessonId);
  const steps = lesson?.steps ?? [];
  const index = Math.min(stepIndex, Math.max(steps.length - 1, 0));
  const step = steps[index];
  const last = index === steps.length - 1;
  const following = lesson ? nextLesson(lesson.id) : undefined;

  // The game as it was when this step began, for checks that look for a change.
  const stepKey = `${lessonId}/${index}`;
  const start = useRef<{ key: string; context: TutorialContext } | null>(null);
  if (start.current?.key !== stepKey) {
    start.current = { key: stepKey, context: now };
  }
  const done = step?.done?.(now, start.current.context) ?? false;

  useHighlight(step != null && !done ? step.target : undefined);

  useEffect(() => {
    if (!done || last) return undefined;
    const timer = window.setTimeout(() => dispatch(increment()), ADVANCE_MS);
    return () => window.clearTimeout(timer);
  }, [done, last, stepKey, dispatch]);

  useEffect(() => {
    if (lesson != null && last) markFinished(lesson.id);
  }, [lesson, last]);

  if (lesson == null || step == null) {
    return (
      <CornerPanel id="tutorial-coach" title="Tutorial" className="coach">
        <p>This lesson is no longer available.</p>
        <button type="button" className="blue-button" onClick={leaveLesson}>
          Back to lessons
        </button>
      </CornerPanel>
    );
  }

  return (
    <CornerPanel id="tutorial-coach" title={`Tutorial: ${lesson.title}`} className="coach">
      <div className="coach-progress">
        Step {index + 1} of {steps.length}
      </div>
      <h3 className="coach-title">{step.title}</h3>
      <div className="coach-body">{step.body}</div>
      {step.task != null && (
        <div className={done ? "coach-task coach-task-done" : "coach-task"} role="status">
          <span className="coach-task-mark" aria-hidden="true">
            {done ? "✓" : "▶"}
          </span>
          <span>{step.task}</span>
        </div>
      )}
      <div className="coach-buttons">
        <button type="button" className="blue-button" disabled={index === 0} onClick={() => dispatch(decrement())}>
          Back
        </button>
        {!last &&
          (step.done != null && !done ? (
            <button
              type="button"
              className="coach-skip"
              title="Move on without doing this step"
              onClick={() => dispatch(increment())}>
              Skip
            </button>
          ) : (
            <button type="button" className="blue-button" onClick={() => dispatch(increment())}>
              Next
            </button>
          ))}
        {last && following != null && (
          <button
            type="button"
            className="blue-button"
            onClick={() => {
              leaveLesson();
              launchLesson(following);
            }}>
            Next lesson: {following.title}
          </button>
        )}
        {last && (
          <button type="button" className="blue-button" onClick={leaveLesson}>
            Back to lessons
          </button>
        )}
      </div>
    </CornerPanel>
  );
}
