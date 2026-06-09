import * as React from "react";
import { useState, useRef, useEffect, useCallback } from "react";
import { GiRocket, GiRingedPlanet } from "react-icons/gi";
import { FaPencilAlt, FaTrash } from "react-icons/fa";

import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";
import { removeEntity, renameEntity } from "lib/serverManager";

type EntityKind = "ship" | "planet";

interface EntityListItem {
  name: string;
  kind: EntityKind;
}

/**
 * Scenario-builder list of all ships and planets in the active scenario.
 *
 * Each row shows a small kind-icon, the name, a pencil (rename), and a red
 * trashcan (delete). Rename opens an inline input; Enter commits via the
 * `RenameEntity` server message, Escape cancels. The trashcan dispatches
 * `Remove`. The server replies with a fresh `EntityResponse` either way, so
 * Redux state stays in sync automatically.
 */
export function EntityList() {
  const entities = useAppSelector(entitiesSelector);
  const [editingName, setEditingName] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const items: EntityListItem[] = React.useMemo(() => {
    const ships: EntityListItem[] = entities.ships.map((s) => ({
      name: s.name,
      kind: "ship" as const,
    }));
    const planets: EntityListItem[] = entities.planets.map((p) => ({
      name: p.name,
      kind: "planet" as const,
    }));
    // Stable display order: ships first, planets second, alphabetical inside
    // each group. Keeps the list from re-shuffling when entities update.
    ships.sort((a, b) => a.name.localeCompare(b.name));
    planets.sort((a, b) => a.name.localeCompare(b.name));
    return [...ships, ...planets];
  }, [entities.ships, entities.planets]);

  // If the entity being edited gets renamed or deleted out from under us
  // (server reply lands while we're still in edit mode), drop the editor.
  useEffect(() => {
    if (editingName && !items.some((i) => i.name === editingName)) {
      setEditingName(null);
      setDraftName("");
    }
  }, [items, editingName]);

  // Focus the input when entering edit mode.
  useEffect(() => {
    if (editingName && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [editingName]);

  const startEdit = useCallback((name: string) => {
    setEditingName(name);
    setDraftName(name);
  }, []);

  const cancelEdit = useCallback(() => {
    setEditingName(null);
    setDraftName("");
  }, []);

  const commitEdit = useCallback(() => {
    if (editingName === null) return;
    const trimmed = draftName.trim();
    if (trimmed === "" || trimmed === editingName) {
      cancelEdit();
      return;
    }
    renameEntity(editingName, trimmed);
    // Drop edit mode immediately; the EntityResponse will land shortly and
    // refresh the list. If the rename fails the server's Error message will
    // surface there.
    cancelEdit();
  }, [editingName, draftName, cancelEdit]);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter") {
        e.preventDefault();
        commitEdit();
      } else if (e.key === "Escape") {
        e.preventDefault();
        cancelEdit();
      }
    },
    [commitEdit, cancelEdit],
  );

  if (items.length === 0) {
    return null;
  }

  return (
    <div className="entity-list">
      <h2 className="entity-list-title">Scenario contents</h2>
      <ul className="entity-list-items">
        {items.map((item) => {
          const Icon = item.kind === "ship" ? GiRocket : GiRingedPlanet;
          const isEditing = editingName === item.name;
          return (
            <li key={`${item.kind}-${item.name}`} className="entity-list-row">
              <Icon
                className="entity-list-kind-icon"
                aria-label={item.kind}
                title={item.kind}
              />
              {isEditing ? (
                <input
                  ref={inputRef}
                  className="entity-list-name-input"
                  value={draftName}
                  onChange={(e) => setDraftName(e.target.value)}
                  onKeyDown={onKeyDown}
                  onBlur={commitEdit}
                  aria-label={`Rename ${item.name}`}
                />
              ) : (
                <span className="entity-list-name">{item.name}</span>
              )}
              <button
                type="button"
                className="entity-list-icon-button"
                onClick={() => (isEditing ? commitEdit() : startEdit(item.name))}
                aria-label={`Rename ${item.name}`}
                title="Rename"
              >
                <FaPencilAlt />
              </button>
              <button
                type="button"
                className="entity-list-icon-button entity-list-delete"
                onClick={() => removeEntity(item.name)}
                aria-label={`Delete ${item.name}`}
                title="Delete"
              >
                <FaTrash />
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export default EntityList;
