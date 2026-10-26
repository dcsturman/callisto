import * as React from "react";
import { useState } from "react";
import { LESSONS } from "lib/tutorial/lessons";
import { launchLesson } from "lib/tutorial/launch";
import { finishedLessons } from "lib/tutorial/progress";

/** The tutorial's lessons, to start any of them; finished ones are ticked. */
export function LessonList({ onBack }: { onBack: () => void }) {
  const [finished] = useState(() => new Set(finishedLessons()));
  return (
    <>
      <div className="authentication-blurb">
        <h1 className="authentication-title">Tutorial</h1>
        <p>
          Short lessons, each in a little scenario of its own. Start at the top, or jump to whatever you want to
          learn. A coach on the right walks you through each one.
        </p>
      </div>
      <ol className="lesson-list">
        {LESSONS.map((lesson, i) => (
          <li key={lesson.id} className="lesson-list-item">
            <span className="lesson-number">{i + 1}</span>
            <span className="lesson-text">
              <span className="lesson-title">
                {lesson.title}
                {finished.has(lesson.id) && (
                  <span className="lesson-finished" title="You have finished this lesson">
                    {" ✓"}
                  </span>
                )}
              </span>
              <span className="lesson-summary">{lesson.summary}</span>
            </span>
            <button type="button" className="blue-button" onClick={() => launchLesson(lesson)}>
              {finished.has(lesson.id) ? "Again" : "Start"}
            </button>
          </li>
        ))}
      </ol>
      <button type="button" className="blue-button tutorial-button" onClick={onBack}>
        Back
      </button>
    </>
  );
}
