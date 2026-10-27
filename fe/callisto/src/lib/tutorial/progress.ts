/**
 * Which lessons this browser has finished, so the lesson list can tick them.
 * Only a convenience: storage can be missing or blocked, and then nothing is
 * ticked.
 */
const KEY = "tutorial-finished";

export function finishedLessons(): string[] {
  try {
    const value = JSON.parse(window.localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}

export function markFinished(id: string) {
  try {
    const done = new Set(finishedLessons());
    done.add(id);
    window.localStorage.setItem(KEY, JSON.stringify([...done]));
  } catch {
    // Not remembered.
  }
}
