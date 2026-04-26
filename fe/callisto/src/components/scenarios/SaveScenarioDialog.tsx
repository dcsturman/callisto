import * as React from "react";
import { useEffect, useState } from "react";
import { saveScenario, SaveScenarioResult } from "lib/serverManager";

interface SaveScenarioDialogProps {
  initialName: string;          // on-disk filename (e.g. "planetfun.json")
  initialDisplayName: string;   // human-readable label (e.g. "Fun with a planet")
  initialDescription: string;
  onClose: () => void;
}

type DialogState =
  | { kind: "editing" }
  | { kind: "saving" }
  | { kind: "confirm-overwrite"; pendingName: string }
  | { kind: "error"; message: string }
  | { kind: "saved"; filename: string };

// Modal dialog driving the save flow. Three fields:
//   Name         — on-disk filename; what existence/ownership checks key off
//   Display Name — what the picker shows; lands in metadata.name
//   Description  — lands in metadata.description
// SCENARIO_EXISTS error → confirm overlay → resubmit with force=true.
// NOT_OWNER and other errors surface as terminal error states.
export function SaveScenarioDialog({
  initialName,
  initialDisplayName,
  initialDescription,
  onClose,
}: SaveScenarioDialogProps) {
  const [name, setName] = useState(initialName);
  const [displayName, setDisplayName] = useState(initialDisplayName);
  const [description, setDescription] = useState(initialDescription);
  const [state, setState] = useState<DialogState>({ kind: "editing" });

  useEffect(() => {
    if (state.kind !== "saved") return;
    const t = setTimeout(onClose, 1200);
    return () => clearTimeout(t);
  }, [state, onClose]);

  function attemptSave(force: boolean) {
    setState({ kind: "saving" });
    saveScenario(
      name.trim(),
      displayName.trim(),
      description,
      force,
      (result: SaveScenarioResult) => {
        if (result.ok) {
          setState({ kind: "saved", filename: result.filename });
          return;
        }
        if (result.error === "SCENARIO_EXISTS") {
          setState({ kind: "confirm-overwrite", pendingName: name });
          return;
        }
        if (result.error.startsWith("NOT_OWNER:")) {
          const owner = result.error.slice("NOT_OWNER:".length);
          setState({
            kind: "error",
            message: `That scenario is owned by ${owner}. You cannot overwrite it.`,
          });
          return;
        }
        setState({ kind: "error", message: result.error });
      },
    );
  }

  function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!name.trim()) {
      setState({ kind: "error", message: "Name is required." });
      return;
    }
    attemptSave(false);
  }

  return (
    <div className="save-scenario-dialog-backdrop">
      <div className="save-scenario-dialog">
        <h2>Save Scenario</h2>
        {state.kind === "saved" ? (
          <p>Saved as <code>{state.filename}</code>.</p>
        ) : state.kind === "confirm-overwrite" ? (
          <>
            <p>
              A scenario file <strong>{state.pendingName}</strong> already
              exists. Overwrite it?
            </p>
            <div className="save-scenario-button-row">
              <button
                type="button"
                onClick={() => setState({ kind: "editing" })}
              >
                Cancel
              </button>
              <button
                type="button"
                className="blue-button"
                onClick={() => attemptSave(true)}
              >
                Overwrite
              </button>
            </div>
          </>
        ) : state.kind === "error" ? (
          <>
            <p className="save-scenario-error">{state.message}</p>
            <div className="save-scenario-button-row">
              <button type="button" onClick={() => setState({ kind: "editing" })}>
                Back
              </button>
              <button type="button" onClick={onClose}>
                Close
              </button>
            </div>
          </>
        ) : (
          <form onSubmit={handleSubmit}>
            <label className="save-scenario-field">
              <span>Name (filename)</span>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                disabled={state.kind === "saving"}
                placeholder="my-scenario"
                autoFocus
              />
            </label>
            <label className="save-scenario-field">
              <span>Display Name</span>
              <input
                type="text"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
                disabled={state.kind === "saving"}
                placeholder="My Scenario"
              />
            </label>
            <label className="save-scenario-field">
              <span>Description</span>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                disabled={state.kind === "saving"}
                rows={4}
              />
            </label>
            <div className="save-scenario-button-row">
              <button
                type="button"
                onClick={onClose}
                disabled={state.kind === "saving"}
              >
                Cancel
              </button>
              <button
                type="submit"
                className="blue-button"
                disabled={state.kind === "saving"}
              >
                {state.kind === "saving" ? "Saving…" : "Save"}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
