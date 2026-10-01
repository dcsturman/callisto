import * as React from "react";

/**
 * A section heading inside a panel, with an icon where one says the thing
 * faster than the word does.
 *
 * The icon is never the whole heading. A crosshair plainly means gunnery and
 * a spanner plainly means repair, but "squadron picture" and "contact detail"
 * have no glyph anyone would read the same way twice, and a console full of
 * guessable pictures is worse than one with words in it. So the word stays
 * and the icon is there to find it by.
 */
export function SectionTag(args: {icon?: React.ReactNode; children: React.ReactNode}) {
  return (
    <div className="section-tag">
      {args.icon != null && (
        <span className="section-tag-icon" aria-hidden="true">
          {args.icon}
        </span>
      )}
      {args.children}
    </div>
  );
}

export default SectionTag;
