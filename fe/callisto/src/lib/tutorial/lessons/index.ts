import { Lesson } from "lib/tutorial/lesson";
import { gettingAround } from "lib/tutorial/lessons/gettingAround";
import { flying } from "lib/tutorial/lessons/flying";
import { sensors } from "lib/tutorial/lessons/sensors";
import { guns } from "lib/tutorial/lessons/guns";
import { missiles } from "lib/tutorial/lessons/missiles";
import { damage } from "lib/tutorial/lessons/damage";
import { captain } from "lib/tutorial/lessons/captain";
import { jumping } from "lib/tutorial/lessons/jumping";
import { scenario } from "lib/tutorial/lessons/scenario";
import { fleet } from "lib/tutorial/lessons/fleet";
import { builder } from "lib/tutorial/lessons/builder";

/** Every lesson, in the order they are taught. Each lives in lessons/. */
export const LESSONS: Lesson[] = [
  gettingAround,
  flying,
  sensors,
  guns,
  missiles,
  damage,
  captain,
  jumping,
  scenario,
  fleet,
  builder,
];

export const lessonById = (id: string | null): Lesson | undefined => LESSONS.find((lesson) => lesson.id === id);

/** The lesson after this one, if there is one. */
export const nextLesson = (id: string): Lesson | undefined => LESSONS[LESSONS.findIndex((l) => l.id === id) + 1];
