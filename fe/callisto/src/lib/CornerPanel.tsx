import * as React from "react";
import { ReactNode, useState } from "react";

/** Read a panel's remembered open state; storage can be absent or blocked. */
const remembered = (id: string, fallback: boolean): boolean => {
  try {
    const value = window.localStorage.getItem(`panel-open:${id}`);
    return value == null ? fallback : value === "1";
  } catch {
    return fallback;
  }
};

const remember = (id: string, open: boolean) => {
  try {
    window.localStorage.setItem(`panel-open:${id}`, open ? "1" : "0");
  } catch {
    // Not remembered; it still toggles.
  }
};

/**
 * A panel in the right-hand column: a header that is always there, with a
 * toggle that hides or shows what is under it, so a player can make room for
 * whatever they need. Its contents scroll inside it rather than pushing the
 * panels below off the screen. Open or shut is remembered per panel, in this
 * browser.
 */
export function CornerPanel({
  id,
  title,
  className,
  initialOpen = true,
  extra,
  children,
}: {
  id: string;
  title: string;
  className?: string;
  initialOpen?: boolean;
  /** Anything to show in the header beside the title. */
  extra?: ReactNode;
  children?: ReactNode;
}) {
  const [open, setOpen] = useState(() => remembered(id, initialOpen));
  const toggle = () => {
    remember(id, !open);
    setOpen(!open);
  };
  return (
    <div id={id} className={`corner-panel ${className ?? ""} ${open ? "" : "corner-panel-shut"}`}>
      <h2 className="corner-panel-header">
        <button
          type="button"
          className="corner-panel-toggle"
          aria-expanded={open}
          onClick={toggle}
          title={open ? `Hide ${title}` : `Show ${title}`}>
          {open ? "▾" : "▸"} {title}
        </button>
        {extra}
      </h2>
      {open && <div className="corner-panel-body">{children}</div>}
    </div>
  );
}
